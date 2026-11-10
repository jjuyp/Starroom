# Responsive Library grid and drawer regression

## Actual product failure

Windows UI run42823518779 ata724a02 successfully attached to production WebView2,
imported a public fixture and reached real controls. Its W3C Search click was
intercepted by the Library sidebar. The captured screenshot showed a blank main
area. This is a product CSS failure, not a driver/runner failure. The elevated CI
browser policy was applied and cleaned successfully; no user registry was touched.

At compact widths Dark CSS turns the sidebar into an absolute drawer and makes
the left grid track zero width. Auto-placement then put the Library main section
in that empty track. Search was at x35 behind the sidebar. Explicit grid tracks
now keep sidebar/main/inspector in columns1/2/3 and row1, including conditional
sidebar and inspector states. Gray/Light also had a separate compact-width rule
that hid the sidebar even while React said open; they now honor the same real
open/close state and use a floating drawer instead of unconditional hiding.

No photo processing, color, resolution or Native graph change is involved.

## Regression evidence

Use the actual production React build, no Native/IPC mocks. The headless Chromium
layout probe uses real theme selection and the real panel keyboard command. It
tests48 combinations: Dark/Gray/Light ×920/1030/1280/1920px ×metadata inspector
closed/open ×sidebar closed/open. Main width/height, explicit columns, physical
Search hit testing and drawer visibility/state must all agree. Reinstating old
auto-placement reproduces contentWidth0 and interception by `library-panel`;
restoring current production CSS passes all48 cases. A transient screenshot was
recaptured after real transition completion, not by removing animation semantics.

Artifacts: `output/playwright/workspace-layout-verified-20261111.json` and
`output/playwright/workspace-layout.png`. Reproduce using the already-approved
Playwright CLI: `run-code --filename scripts/fixtures/workspace-layout-browser.mjs`
against the production page in Library. The probe is layout-only; Browser demo
mode remains read-only and is not image-processing or Native functionality proof.
The first Unicode-path Vite dev navigation timed out; the qualification instead
used the canonical fresh ASCII-alias production build and static preview server.

FullWeb245/35files report1794330185706, TypeScript/lint and fresh production build
pass. Actual Windows Native workflow now asserts Library physical usability
before import, retains real W3C clicks, and still requires each of six real tone
controls to change Native pixels and exact Undo/Redo. UI-critical App/CSS/queue/
presentation source changes trigger that supplementary workflow; installer and
final release gates are unchanged. Same-SHA Windows acceptance remains pending.
