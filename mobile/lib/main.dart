import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:kasadesk_protocol/relay_account.dart';
import 'package:url_launcher/url_launcher.dart';
import 'connection.dart';
import 'home.dart';

const paper = Color(0xfff5f4ee),
    ink = Color(0xff20302e),
    teal = Color(0xff226750);
void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const KasaDesk());
}

ThemeData deskTheme() => ThemeData(
  useMaterial3: true,
  scaffoldBackgroundColor: paper,
  colorScheme: ColorScheme.fromSeed(seedColor: teal, surface: paper),
  appBarTheme: const AppBarTheme(backgroundColor: paper, foregroundColor: ink),
  inputDecorationTheme: const InputDecorationTheme(
    filled: true,
    fillColor: Colors.white,
    border: OutlineInputBorder(
      borderRadius: BorderRadius.all(Radius.circular(14)),
      borderSide: BorderSide.none,
    ),
  ),
);
const store = FlutterSecureStorage();
const gateway = String.fromEnvironment(
  'DESK_GATEWAY',
  defaultValue: defaultGateway,
);

const accountStorageKey = 'desk-account@$gateway';

class KasaDesk extends StatefulWidget {
  const KasaDesk({super.key});
  @override
  State<KasaDesk> createState() => _KasaDeskState();
}

class _KasaDeskState extends State<KasaDesk> {
  DeskConnection? link;
  bool loading = true;
  int epoch = 0;
  @override
  void initState() {
    super.initState();
    restore();
  }

  Future<void> restore() async {
    try {
      final raw = await store.read(key: accountStorageKey);
      if (raw != null) {
        final c = AccountSession.fromJson(jsonDecode(raw));
        if (c.origin == Uri.parse(gateway) && mounted) attach(c);
      }
    } catch (_) {
      /* The login form remains usable after a keychain reset. */
    }
    if (mounted) setState(() => loading = false);
  }

  void attach(AccountSession c) {
    final next = DeskConnection(c);
    link = next;
    epoch++;
    void expired() {
      if (identical(link, next)) unawaited(signOut(revoke: false));
    }

    next.expired = expired;
    next.server.onUnauthorized = expired;
  }

  Future<void> signedIn(AccountSession c) async {
    await store.write(key: accountStorageKey, value: jsonEncode(c.toJson()));
    if (mounted) setState(() => attach(c));
  }

  Future<void> signOut({bool revoke = true}) async {
    final old = link;
    if (old == null) return;
    old.close();
    setState(() {
      link = null;
      loading = true;
      epoch++;
    });
    try {
      await store.delete(key: accountStorageKey);
    } finally {
      if (mounted) setState(() => loading = false);
    }
    if (revoke) {
      final api = RelayAccountApi(old.session.origin, session: old.session);
      try {
        await api.logout();
      } catch (_) {
      } finally {
        api.close();
      }
    }
  }

  @override
  Widget build(BuildContext c) => MaterialApp(
    key: ValueKey(epoch),
    debugShowCheckedModeBanner: false,
    title: 'KasaDesk',
    theme: deskTheme(),
    home: loading
        ? const Scaffold(body: Center(child: CircularProgressIndicator()))
        : link == null
        ? Login(onLogin: signedIn)
        : DeskHome(link: link!, logout: signOut),
  );
}

class Login extends StatefulWidget {
  const Login({super.key, required this.onLogin});
  final Future<void> Function(AccountSession) onLogin;
  @override
  State<Login> createState() => _LoginState();
}

class _LoginState extends State<Login> {
  final id = TextEditingController(), pw = TextEditingController();
  late final api = RelayAccountApi(
    Uri.parse(gateway),
    client: DeskLoginClient(Uri.parse(gateway), installId),
  );
  List<OAuthProvider> providers = [];
  OAuthFlow? flow;
  OAuthChoice? choice;
  String? error;
  bool busy = false;
  @override
  void initState() {
    super.initState();
    api
        .oauthProviders()
        .then((p) {
          if (mounted) setState(() => providers = p.enabled);
        })
        .catchError((_) {});
  }

  @override
  void dispose() {
    id.dispose();
    pw.dispose();
    api.close();
    super.dispose();
  }

  Future<void> act(Future<void> Function() fn) async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await fn();
    } catch (e) {
      if (mounted) {
        setState(
          () => error = e is AccountException
              ? e.message
              : '연결을 확인하고 다시 시도해 주세요.',
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<String> installId() async {
    var install = await store.read(key: 'desk-install@$gateway');
    if (install == null) {
      final rng = Random.secure();
      install = List.generate(
        16,
        (_) => rng.nextInt(256).toRadixString(16).padLeft(2, '0'),
      ).join();
      await store.write(key: 'desk-install@$gateway', value: install);
    }
    return install;
  }

  Future<void> oauth(OAuthProvider p) async {
    final f = await api.oauthStart(p, await installId());
    if (!mounted) return;
    setState(() => flow = f);
    try {
      if (!await launchUrl(
        f.authorization,
        mode: LaunchMode.externalApplication,
      )) {
        throw const AccountException('브라우저를 열 수 없어요.');
      }
      while (mounted && flow == f && DateTime.now().isBefore(f.expires)) {
        await Future<void>.delayed(const Duration(seconds: 2));
        if (!mounted || flow != f) return;
        final r = await api.oauthPoll(f);
        if (!mounted || flow != f) return;
        if (r.session != null) {
          await widget.onLogin(r.session!);
          return;
        }
        if (r.choice != null) {
          setState(() => choice = r.choice);
          return;
        }
      }
      if (mounted && flow == f) throw const AccountException('로그인 시간이 지났어요.');
    } finally {
      if (mounted && flow == f) setState(() => flow = null);
    }
  }

  @override
  Widget build(BuildContext c) => Scaffold(
    body: SafeArea(
      child: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(24),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                ClipRRect(
                  borderRadius: BorderRadius.circular(22),
                  child: Stack(
                    alignment: Alignment.bottomLeft,
                    children: [
                      AspectRatio(
                        aspectRatio: 2.2,
                        child: Image.asset(
                          'assets/river.jpg',
                          fit: BoxFit.cover,
                        ),
                      ),
                      Positioned.fill(
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            gradient: LinearGradient(
                              begin: Alignment.topCenter,
                              end: Alignment.bottomCenter,
                              colors: [Colors.transparent, Color(0xbd192d29)],
                            ),
                          ),
                        ),
                      ),
                      const Padding(
                        padding: EdgeInsets.all(20),
                        child: Text(
                          'KasaDesk',
                          style: TextStyle(
                            fontSize: 34,
                            fontWeight: FontWeight.w700,
                            color: Colors.white,
                            letterSpacing: -1.5,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 32),
                if (choice != null) Text('계정 연결 · ${choice!.display}'),
                if (flow != null) ...[
                  SelectableText(
                    '브라우저 확인 코드  ${flow!.userCode}',
                    textAlign: TextAlign.center,
                  ),
                  TextButton(
                    onPressed: () {
                      final f = flow!;
                      setState(() => flow = null);
                      unawaited(api.oauthCancel(f).catchError((_) {}));
                    },
                    child: const Text('취소'),
                  ),
                ] else ...[
                  if (choice == null && providers.isNotEmpty) ...[
                    Wrap(
                      alignment: WrapAlignment.center,
                      spacing: 8,
                      children: [
                        for (final p in providers)
                          OutlinedButton(
                            onPressed: busy ? null : () => act(() => oauth(p)),
                            child: Text('${p.label}로 로그인'),
                          ),
                      ],
                    ),
                    const SizedBox(height: 16),
                  ],
                  TextField(
                    controller: id,
                    autocorrect: false,
                    decoration: const InputDecoration(labelText: '카사계정 아이디'),
                    textInputAction: TextInputAction.next,
                  ),
                  const SizedBox(height: 10),
                  TextField(
                    controller: pw,
                    obscureText: true,
                    enableSuggestions: false,
                    autocorrect: false,
                    decoration: const InputDecoration(labelText: '비밀번호'),
                  ),
                  const SizedBox(height: 16),
                  FilledButton(
                    onPressed: busy
                        ? null
                        : () => act(() async {
                            final s = choice == null
                                ? await api.login(id.text, pw.text)
                                : await api.oauthClaim(
                                    choice!,
                                    id.text,
                                    pw.text,
                                  );
                            if (mounted) await widget.onLogin(s);
                          }),
                    child: Text(
                      busy
                          ? '연결 중…'
                          : choice == null
                          ? '로그인'
                          : '기존 계정에 연결',
                    ),
                  ),
                  if (choice?.signup == true)
                    TextButton(
                      onPressed: busy
                          ? null
                          : () => act(() async {
                              await widget.onLogin(
                                await api.oauthSignup(choice!),
                              );
                            }),
                      child: const Text('새 계정 만들기'),
                    ),
                ],
                if (error != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Text(
                      error!,
                      style: TextStyle(color: Theme.of(c).colorScheme.error),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}
