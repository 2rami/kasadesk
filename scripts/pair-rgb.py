"""Pair one KasaDesk roster device with an installed RGB Control SSH bridge."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def default_config():
    if sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    elif os.name == "nt":
        base = Path(os.environ["LOCALAPPDATA"])
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    return base / "KasaDesk" / "rgb.json"


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("ssh_host", help="OpenSSH host alias for the RGB Control PC")
    p.add_argument("device_id", help="its full KasaNet node id from roster.json")
    p.add_argument("--output", type=Path, default=default_config())
    args = p.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", args.ssh_host) or not re.fullmatch(r"[0-9a-fA-F]{64}", args.device_id):
        p.error("expected an SSH alias and a 64-character hexadecimal device id")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    raw = subprocess.check_output(["ssh", "-T", "-oBatchMode=yes", "-oStrictHostKeyChecking=yes",
                                   "-oForwardAgent=no", "-oClearAllForwardings=yes", "-oConnectTimeout=8",
                                   args.ssh_host, "python -I -u .kasadesk-rgb/bridge.py pair"], timeout=20)
    grant = json.loads(raw)
    if grant.get("v") != 1 or not re.fullmatch(r"[A-Za-z0-9_-]{43}", grant.get("token", "")):
        p.error("invalid bridge reply")
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=args.output.parent, delete=False) as f:
        json.dump({"ssh_host": args.ssh_host, "device_id": args.device_id.lower(), "token": grant["token"]}, f)
    os.replace(f.name, args.output)
    print(f"Paired RGB Control. Configuration: {args.output}")
    print("Grant ID:", hashlib.sha256(grant["token"].encode("ascii")).hexdigest())


if __name__ == "__main__":
    main()
