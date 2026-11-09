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

First real run42625884494 reached the built executable but failed before any tone
operation: EdgeDriver's DevToolsActivePort negotiation did not complete. This is
a harness setup failure, not product tone failure or external runner outage.
Review also identified that bare cargo build lacks Tauri's custom-protocol feature
and therefore is not sufficient standalone-frontend evidence. The next run uses
the pinned Tauri CLI build --no-bundle (its production-protocol feature), an owned
application/profile with loopback WebView2 debugging, and Microsoft's documented
attach approach. It requires a production tauri origin, not a development server.
One PageUp input per control matches one physical History gesture; four separate
arrow presses cannot be asserted to undo as a single drag. Completed native builds
will be cached even when a later UI assertion fails, avoiding repeated cold builds.
The revised transport/guard tests pass4 cases. This remains unaccepted until the
real workflow passes.
Full local Vitest241/35files and TypeScript pass for the batched repair.
d4ff25f Full42625884357/secondary42625892911 succeeded independently; its
green Rust/Web checks do not override the failed real UI workflow.

## Previous performance group

1d7e114 Full42621317707/secondary42621327038 SUCCESS. Local FullRust1794228710901,
FullWeb1794228773739 (237 tests), and actual D750 RAW157 same-tier GPU/CPU/export
comparisons1794229651092 pass(maxRGB8delta1). At512, exposure median/p9515.075/
17.611ms, shadows16.889/18.862ms, Tone+NR+Detail17.558/18.374ms; Native only.
The local MSVC release-profile executable headless self-test passed Library/History/
Session/NativeExport with deterministic outputs and immutable source in
output/qualification/core-2LWMwq/qualification.json, executableSHA256
23eee0149c2c91c20d49b893e154f25b96c4a74b79a194ca59debc6e8531fee9.
It is not standalone UI-protocol, installed/AI-bundling or whole-UI acceptance.
The same executable also completed real CPU Subject inference, pinned modelSHA
5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333,
with finite1024x1024 model-resolution output, exit0. Saved evidence:
output/qualification/ai-verified-VyiZgl/ai-qualification.json.
This does not validate DirectML, photographic mask quality or installed availability.
