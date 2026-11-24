#!/usr/bin/env python3
"""KasaDesk 릴리스 — 맥 릴리스 기계에서 돈다(개발 맥이 먼저, 맥미니가 예비). 카사텀 미리보기 배포와 같은 길이다.

  python3 scripts/release.py [커밋]      # 기본은 origin/main 끝

1. 판 번호를 정한다(마지막 태그의 다음 조각, 처음이면 Cargo.toml 판) — 다르면 판 커밋을 main 에 올린다.
2. 맥: Developer ID·hardened runtime 으로 굽고 dmg 를 만들어 공증·staple 한다.
3. 태그와 GitHub 릴리스(dmg)를 만들고, Windows 굽기(.github/workflows/release.yml)를 그 태그로 불러 MSI 를 붙이게 한다.
4. dmg·MSI 에 이 앱 전용 EdDSA 키로 서명해 docs/appcast.xml·appcast-win.xml 을 main 에 올리고, Pages 가 새 판을 내줄 때까지 기다린다.

Windows MSI 는 저장소 변수 WINDOWS_RUNNER(집 PC 자체 러너 kasa-home)가 굽는다. 한 번에 한 기계에서만 돌린다 — 두 곳에서
동시에 돌리면 판 커밋이 엇갈린다.

비밀값은 릴리스 기계에만 있다 — EdDSA 개인키(~/.config/kasadesk-release/, 개발 맥·맥미니·집 PC 에 같은 키), Developer ID 가 든
열쇠고리, 공증 프로필(AC_NOTARY). 저장소·로그에는 남기지 않는다.
"""

import argparse
import calendar
import email.utils
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

REPO = "2rami/kasadesk"
REMOTE = f"https://github.com/{REPO}.git"
PAGES = "https://2rami.github.io/kasadesk"
HOME = Path.home()
WORK = HOME / ".local/share/kasadesk-release"
ED_KEY = HOME / ".config/kasadesk-release/sparkle-ed25519.key"
SIGN_UPDATE = WORK / "Sparkle/bin/sign_update"
# 맥미니는 서명 전용 열쇠고리를 따로 두고, 개발 맥은 로그인 열쇠고리에 Developer ID 가 있다.
KEYCHAIN = next((k for k in (HOME / "Library/Keychains/codesign.keychain-db", HOME / "Library/Keychains/login.keychain-db")
                 if k.exists()), HOME / "Library/Keychains/codesign.keychain-db")
UNLOCK = HOME / "bin/unlock-signing"
NOTARY_PROFILE = "AC_NOTARY"
TEAM = "L366799VND"
GH = shutil.which("gh") or str(HOME / ".local/bin/gh")
MIN_MACOS = "13.0"


class Stop(Exception):
    pass


def run(*argv, cwd=None, env=None, timeout=3600, quiet=False):
    r = subprocess.run([str(a) for a in argv], cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if r.returncode != 0:
        tail = "\n".join((r.stderr or r.stdout).strip().splitlines()[-6:])
        raise Stop(f"{Path(str(argv[0])).name} {' '.join(str(a) for a in argv[1:3])} 실패:\n{tail}")
    if not quiet and r.stdout.strip():
        print(r.stdout.strip().splitlines()[-1])
    return r.stdout


def git(*args, cwd):
    return run("git", *args, cwd=cwd, quiet=True).strip()


def log(text):
    print(f"[release] {text}", flush=True)


def version_key(v):
    return tuple(int(p) for p in v.split("."))


def next_version(repo):
    current = re.search(r'^version = "(.+)"$', (repo / "Cargo.toml").read_text(), re.M).group(1)
    tags = [t[1:] for t in git("tag", "-l", "v*", cwd=repo).split() if re.fullmatch(r"v\d+\.\d+\.\d+", t)]
    if not tags:
        return current, None
    last = max(tags, key=version_key)
    if version_key(current) > version_key(last):
        return current, last
    major, minor, patch = version_key(last)
    return f"{major}.{minor}.{patch + 1}", last


def set_version(repo, version):
    cargo = repo / "Cargo.toml"
    text = cargo.read_text()
    cargo.write_text(re.sub(r'^version = ".+"$', f'version = "{version}"', text, count=1, flags=re.M))
    lock = repo / "Cargo.lock"
    text = lock.read_text()
    text, n = re.subn(r'(\[\[package\]\]\nname = "kasadesk"\nversion = )"[^"]+"', rf'\1"{version}"', text)
    if n != 1:
        raise Stop("Cargo.lock 에서 kasadesk 판을 못 찾았다")
    lock.write_text(text)


def signing_identity():
    out = run("security", "find-identity", "-v", "-p", "codesigning", KEYCHAIN, quiet=True)
    for line in out.splitlines():
        m = re.search(r'([0-9A-F]{40}) "(Developer ID Application: .+ \((\w+)\))"', line)
        if m and m.group(3) == TEAM:
            return m.group(1)
    raise Stop(f"{KEYCHAIN} 에 팀 {TEAM} 의 Developer ID Application 인증서가 없다")


def notary_keychain():
    """공증 프로필이 든 열쇠고리 — 맥미니는 서명 전용 열쇠고리에 넣었고, 개발 맥은 store-credentials 기본 자리(로그인 파일로
    지정하면 못 찾는다)라 넘기지 않는다."""
    return [] if KEYCHAIN.name == "login.keychain-db" else ["--keychain", KEYCHAIN]


def unlock():
    if os.access(UNLOCK, os.X_OK):
        run(UNLOCK, quiet=True, timeout=60)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def ed_signature(path):
    out = run(SIGN_UPDATE, "--ed-key-file", ED_KEY, path, quiet=True)
    sig = re.search(r'sparkle:edSignature="([^"]+)"', out)
    length = re.search(r'length="(\d+)"', out)
    if not sig or not length or int(length.group(1)) != path.stat().st_size:
        raise Stop(f"{path.name} 서명 출력이 이상하다")
    return sig.group(1), int(length.group(1))


def feed(version, url, signature, length, windows):
    os_attr = ' sparkle:os="windows"' if windows else ""
    minimum = "" if windows else f"\n      <sparkle:minimumSystemVersion>{MIN_MACOS}</sparkle:minimumSystemVersion>"
    return f"""<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>KasaDesk</title>
    <link>{PAGES}/{'appcast-win.xml' if windows else 'appcast.xml'}</link>
    <item>
      <title>{version}</title>
      <pubDate>{email.utils.formatdate(usegmt=True)}</pubDate>
      <sparkle:version>{version}</sparkle:version>
      <sparkle:shortVersionString>{version}</sparkle:shortVersionString>{minimum}
      <enclosure url="{url}"{os_attr} sparkle:edSignature="{signature}" length="{length}" type="application/octet-stream"/>
    </item>
  </channel>
</rss>
"""


def checkout(commit):
    repo = WORK / "repo"
    if not (repo / ".git").exists():
        WORK.mkdir(parents=True, exist_ok=True)
        run("git", "clone", "-q", REMOTE, repo, env={**os.environ, "GIT_LFS_SKIP_SMUDGE": "1"})
    git("fetch", "-q", "--tags", "origin", cwd=repo)
    head = git("rev-parse", "origin/main", cwd=repo)
    commit = git("rev-parse", commit or head, cwd=repo)
    if commit != head:
        raise Stop(f"{commit[:8]} 이 main 끝({head[:8]})이 아니다 — 릴리스는 main 끝에서만 낸다")
    git("checkout", "-q", "-f", "-B", "main", head, cwd=repo)
    git("clean", "-q", "-fdx", "-e", "/target", "-e", "/vendor", cwd=repo)
    env = {**os.environ, "GIT_LFS_SKIP_SMUDGE": "1"}
    run("git", "submodule", "update", "-q", "--init", "--depth", "1", cwd=repo, env=env)
    run("git", "-C", "kasaterm", "lfs", "pull", "--include=crates/kasa-gridview/assets/fonts/*", cwd=repo, quiet=True)
    return repo, head


def bump(repo, version):
    set_version(repo, version)
    git("add", "Cargo.toml", "Cargo.lock", cwd=repo)
    git("commit", "-q", "-m", f"chore(release): v{version}", cwd=repo)
    git("push", "-q", "origin", "HEAD:main", cwd=repo)
    return git("rev-parse", "HEAD", cwd=repo)


def build_mac(repo, version, out):
    identity = signing_identity()
    env = {**os.environ, "KASADESK_SIGN_ID": identity, "KASADESK_SIGN_KEYCHAIN": str(KEYCHAIN), "KASADESK_HARDENED": "1",
           "CARGO_TARGET_DIR": str(WORK / "target")}
    if os.access(UNLOCK, os.X_OK):
        env["KASADESK_SIGN_UNLOCK"] = str(UNLOCK)
    run("bash", "scripts/bundle-mac.sh", cwd=repo, env=env)
    app = repo / "dist/KasaDesk.app"
    run("codesign", "--verify", "--deep", "--strict", app, quiet=True)
    detail = subprocess.run(["codesign", "-dv", str(app)], capture_output=True, text=True).stderr
    if f"TeamIdentifier={TEAM}" not in detail or "runtime" not in detail:
        raise Stop("번들 서명이 팀·hardened runtime 이 아니다")
    stage = out / "stage"
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    run("ditto", app, stage / "KasaDesk.app", quiet=True)
    os.symlink("/Applications", stage / "Applications")
    dmg = out / f"KasaDesk-{version}-macos-arm64.dmg"
    dmg.unlink(missing_ok=True)
    run("hdiutil", "create", "-volname", "KasaDesk", "-srcfolder", stage, "-ov", "-format", "UDZO", dmg, quiet=True)
    shutil.rmtree(stage, ignore_errors=True)
    unlock()
    run("codesign", "--force", "--sign", identity, "--timestamp", "--keychain", KEYCHAIN, dmg, quiet=True)
    log("공증 중")
    unlock()
    # --force: 올리기 전 검사가 dmg 를 붙였다가 개발 맥에서 가끔 영영 못 떼고 멈춘다(2026-11-24 두 번, 50분 시간 초과).
    # 형식은 위 hdiutil·codesign 이 이미 확인했고 Apple 쪽이 다시 본다.
    raw = run("xcrun", "notarytool", "submit", dmg, "--force", "--keychain-profile", NOTARY_PROFILE, *notary_keychain(),
              "--wait", "--timeout", "45m", "--output-format", "json", quiet=True, timeout=3000)
    status = json.loads(raw or "{}")
    if status.get("status") != "Accepted":
        raise Stop(f"공증 거절({status.get('status')}) — xcrun notarytool log {status.get('id')} --keychain-profile {NOTARY_PROFILE}")
    run("xcrun", "stapler", "staple", dmg, quiet=True)
    gate = subprocess.run(["spctl", "-a", "-t", "open", "--context", "context:primary-signature", "-vv", str(dmg)],
                          capture_output=True, text=True)
    if "source=Notarized Developer ID" not in gate.stderr + gate.stdout:
        raise Stop("spctl 이 dmg 를 공증된 Developer ID 로 받지 않는다")
    return dmg


def windows_msi(repo, tag, out):
    dispatched = time.time()
    run(GH, "workflow", "run", "release.yml", "-R", REPO, "--ref", "main", "-f", f"tag={tag}", quiet=True)
    run_id = None
    for _ in range(30):
        time.sleep(10)
        rows = json.loads(run(GH, "run", "list", "-R", REPO, "--workflow", "release.yml", "--event", "workflow_dispatch",
                              "-L", "5", "--json", "databaseId,createdAt", quiet=True))
        fresh = [r for r in rows if calendar.timegm(time.strptime(r["createdAt"], "%Y-%m-%dT%H:%M:%SZ")) >= dispatched - 30]
        if fresh:
            run_id = fresh[0]["databaseId"]
            break
    if not run_id:
        raise Stop("Windows 굽기 실행을 못 찾았다")
    log(f"Windows 굽기 https://github.com/{REPO}/actions/runs/{run_id}")
    run(GH, "run", "watch", str(run_id), "-R", REPO, "--exit-status", quiet=True, timeout=5400)
    for stale in out.glob("KasaDesk-*-windows-x86_64.msi"):
        stale.unlink()
    run(GH, "release", "download", tag, "-R", REPO, "-p", "KasaDesk-*-windows-x86_64.msi", "-D", out, quiet=True)
    found = sorted(out.glob("KasaDesk-*-windows-x86_64.msi"))
    if len(found) != 1:
        raise Stop("릴리스에서 MSI 를 하나로 못 받았다")
    return found[0]


def publish_feeds(repo, version, tag, dmg, msi):
    base = f"https://github.com/{REPO}/releases/download/{tag}"
    mac_sig, mac_len = ed_signature(dmg)
    win_sig, win_len = ed_signature(msi)
    for attempt in range(5):
        git("fetch", "-q", "origin", cwd=repo)
        git("checkout", "-q", "-f", "-B", "main", "origin/main", cwd=repo)
        docs = repo / "docs"
        docs.mkdir(exist_ok=True)
        (docs / ".nojekyll").write_text("")
        (docs / "appcast.xml").write_text(feed(version, f"{base}/{dmg.name}", mac_sig, mac_len, windows=False))
        (docs / "appcast-win.xml").write_text(feed(version, f"{base}/{msi.name}", win_sig, win_len, windows=True))
        git("add", "docs", cwd=repo)
        git("commit", "-q", "-m", f"chore(release): v{version} 피드", cwd=repo)
        if subprocess.run(["git", "push", "-q", "origin", "HEAD:main"], cwd=repo).returncode == 0:
            break
        time.sleep(5)
    else:
        raise Stop("피드 커밋을 main 에 못 올렸다")
    pages = subprocess.run([GH, "api", f"repos/{REPO}/pages"], capture_output=True, text=True)
    if pages.returncode != 0:
        run(GH, "api", "-X", "POST", f"repos/{REPO}/pages", "-f", "source[branch]=main", "-f", "source[path]=/docs", quiet=True)
    for _ in range(60):
        try:
            with urllib.request.urlopen(f"{PAGES}/appcast.xml?t={time.time()}", timeout=20) as r:
                mac = r.read().decode()
            with urllib.request.urlopen(f"{PAGES}/appcast-win.xml?t={time.time()}", timeout=20) as r:
                win = r.read().decode()
            if f"<sparkle:version>{version}</sparkle:version>" in mac and f"<sparkle:version>{version}</sparkle:version>" in win:
                return
        except OSError:
            pass
        time.sleep(15)
    raise Stop("Pages 가 15분 안에 새 피드를 내주지 않았다")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("commit", nargs="?")
    args = ap.parse_args()
    for need in (ED_KEY, SIGN_UPDATE, KEYCHAIN):
        if not Path(need).exists():
            raise Stop(f"{need} 가 없다 — 릴리스 기계 준비가 덜 됐다")
    repo, head = checkout(args.commit)
    version, previous = next_version(repo)
    tag = f"v{version}"
    if git("tag", "-l", tag, cwd=repo):
        raise Stop(f"{tag} 가 이미 있다")
    current = re.search(r'^version = "(.+)"$', (repo / "Cargo.toml").read_text(), re.M).group(1)
    commit = head if current == version else bump(repo, version)
    log(f"{tag} ← {commit[:8]}")
    out = WORK / "out" / tag
    out.mkdir(parents=True, exist_ok=True)
    dmg = build_mac(repo, version, out)
    log(f"맥 {dmg.name} 공증·staple 끝")
    git("tag", tag, commit, cwd=repo)
    git("push", "-q", "origin", tag, cwd=repo)
    span = f"v{previous}..{commit}" if previous else commit
    notes = git("log", "--format=- %s", "--no-merges", span, cwd=repo) or "- 첫 판"
    run(GH, "release", "create", tag, dmg, "-R", REPO, "--title", f"KasaDesk {version}", "--notes", notes, "--verify-tag", quiet=True)
    msi = windows_msi(repo, tag, out)
    log(f"Windows {msi.name}")
    publish_feeds(repo, version, tag, dmg, msi)
    print(json.dumps({"version": version, "tag": tag, "commit": commit,
                      "dmg": {"name": dmg.name, "size": dmg.stat().st_size, "sha256": sha256(dmg)},
                      "msi": {"name": msi.name, "size": msi.stat().st_size, "sha256": sha256(msi)},
                      "feeds": [f"{PAGES}/appcast.xml", f"{PAGES}/appcast-win.xml"]}, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except Stop as e:
        print(f"[release] 멈춤: {e}", file=sys.stderr)
        sys.exit(1)
