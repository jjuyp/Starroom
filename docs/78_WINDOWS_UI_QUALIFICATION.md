# Real Windows UI qualification

2026-11-09. Status: under qualification; real CI execution is required.

## Gap

The existing installer smoke test checks process survival and real core/AI
self-tests, but terminates its UI process without operating tone controls.
Native numerical tests and Chromium presentation tests cannot alone prove that
the actual installed React/WebView2 interface changes a photo or saves History.

The new supplemental Windows UI workflow builds the actual MSVC Release binary,
with the production frontend and Native commands, then uses Microsoft's signed,
Runtime-matched Edge WebDriver directly. No renderer-only browser, IPC mock,
test plugin, alternate graph, WebView remote-server plugin or npm dependency.
Microsoft documents this direct WebView2 launch approach:
[official guide](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/webdriver).
Tauri also supports standard external WebDriver testing:
[official guide](https://v2.tauri.app/develop/tests/webdriver/).

## Safety and scope

- Explicit Windows GitHub runner flags required; local invocations throw before
  launching any application. The user still opens the delivered desktop themselves.
- A fresh owned profile controls APPDATA/LOCALAPPDATA/WebView2 and model discovery.
  Private model overrides are empty, inherited token/secret variables are removed
  from the child environment. Test data is an existing public photographic fixture.
- Driver preparation reuses only a matching valid Microsoft-signed executable or
  obtains the matching driver from Microsoft's HTTPS host. Only the exact root
  executable is extracted to a new path with size bounds; no archive paths,
  overwrite or deletion. Version/hash/signature are recorded and checked again.
- Driver is loopback-only, HTTP operations bounded. Only owned sessions/processes
  are closed. Tool binaries/archives are never uploaded as artifacts or packaged.

## Actual workflow assertions

Native import command -> real Library refresh/select -> Edit/Light -> six tone
controls via actual W3C key input. Each control must change the actual canvas
pixels, restore final-quality dimensions and produce a measured edit update,
not just change a numeric field. Real Ctrl-Z/Ctrl-Shift-Z must reproduce exact
previous/final canvas signatures. A bounded observer catches transient typed
failures, including History commit and poisoned GPU notices, not only final alerts.

This is functional qualification, NOT a complete six-control latency gate,
all-tool UX acceptance, final installed packaging proof or Lightroom comparison.
The first workflow qualifies the production executable before integrating the
same operation into the eventual installer gate; no installed qualification is
claimed until it has actually run on an installed binary.

## Verification state

Local transport/launch-guard tests pass3 cases; they are transport contracts only,
not proof of real application operation. Targeted ESLint, TypeScript, PowerShell
AST and bundled PyYAML syntax validation pass. Actual Windows run remains pending.
The complete frontend test run passes240 tests/35files with TypeScript and the
19-target infrastructure validator. Production frontend source did not change;
the prior237-test group's built frontend and Native qualification remain scoped
evidence, not replaced by the three transport tests.
The independent supplementary workflow is path-targeted; existing/unknown critical
workflow changes retain broad validation and all final release gates are unchanged.

## Previous performance group

1d7e114 Full42621317707/secondary42621327038 SUCCESS. Local FullRust1794228710901,
FullWeb1794228773739 (237 tests), and actual D750 RAW157 same-tier GPU/CPU/export
comparisons1794229651092 pass(maxRGB8delta1). At512, exposure median/p9515.075/
17.611ms, shadows16.889/18.862ms, Tone+NR+Detail17.558/18.374ms; Native only.
The local MSVC production executable headless self-test passed Library/History/
Session/NativeExport with deterministic outputs and immutable source in
output/qualification/core-2LWMwq/qualification.json, executableSHA256
23eee0149c2c91c20d49b893e154f25b96c4a74b79a194ca59debc6e8531fee9.
It is not installed/AI-bundling or whole-UI acceptance.
