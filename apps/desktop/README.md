# Octoii desktop

An Electron desktop app bundling the existing Web interface and connecting to an independent local Octoii node. macOS first; the shell also keeps the standard Windows / Linux window lifecycle.

From the repository root:

```sh
bun install
bun run desktop
```

The desktop uses the installed CLI at `~/.local/bin/octoii` (`OCTOII_BIN` overrides it), with `~/.octoii` as its Home (`OCTOII_HOME` overrides it). It asks `status --json` through the CLI's private control socket and uses the returned address, including a non-default port.

If the default macOS node is offline, it starts the existing launchd service, or installs it through `octoii service install` when no service plist exists. The installed service starts at login and restarts after failure. Existing service definitions are never overwritten. The desktop waits for readiness and reports failure with Retry / Close. A missing CLI, custom Home, or other OS requires manual setup; this milestone does not bundle the executable.

`bun run desktop` builds the current Web interface into `apps/desktop/dist/web` along with the Electron main process. The node no longer needs to serve HTML, JavaScript, CSS or fonts for the desktop to render. In Electron's session, bundled assets are served at the selected node's HTTP origin; API requests are forwarded through the same session, and WebSockets connect directly to the node. This preserves the existing same-origin cookie and client protocol without a second transport or business IPC layer. Ordinary browser access remains unchanged.

Before connecting, a script-free page at `octoii-desktop://startup/` shows progress. Connection or pairing failures display Retry and Close window actions in the window. Retry repeats discovery and pairing; closing the window leaves the independent service running.

On first connection, the desktop requests a link using `octoii pair` and redeems it through the window's Electron session before loading the page. The resulting HttpOnly cookie is persisted and reused. Existing cookies are not silently replaced, including after access is revoked; the existing pairing screen remains available for explicit reauthorization. Pairing credentials are not placed in renderer URLs or logs by this automatic flow.

For a manually managed endpoint, set `OCTOII_DESKTOP_URL` to a loopback HTTP URL or a fresh pairing link. This explicit mode skips CLI discovery, service setup and automatic pairing. Treat pairing links as credentials. Electron keeps its session cookies independently of the browser.

Closing the window or quitting Electron leaves the node and its tasks running. On macOS, the Dock reopens a closed window. Failed initial connections offer Retry / Close. External HTTP(S) links open in the system browser. The renderer is sandboxed, has no Node.js access or preload bridge, and cannot navigate to another origin.

This shell can start the installed node through launchd and bundles Web assets, but does not yet bundle the node executable, provide an application installer, or implement native notifications. The desktop bundle determines the interface version; the connected node must speak the matching protocol. Phone work follows the desktop prototype.

## Verification

`bun run --cwd apps/desktop test` covers discovery without duplicate starts, readiness after a service-start error, invalid status and executable failures, loopback restrictions, pairing-origin checks, and cookie reuse. The desktop participates in the workspace typecheck, lint and test commands.

A macOS Electron integration check also exercised the installed node: discovery, real pairing into an isolated session, HttpOnly cookie establishment and reuse, and sandboxed page loading. Its temporary authorization was revoked afterward. Fresh launchd installation has not been exercised against the user's service configuration; the existing node was left running.

Bundled-resource tests cover local loading, MIME types, API forwarding, path traversal rejection and missing assets; startup messages are HTML-escaped. A separate isolated Electron check used a temporary CLI and HTTP node to verify the failure page, Retry recovery, rendering without backend assets, cookie-bearing API requests and direct WebSocket connections. No user node or service was changed during that check.

The desktop remembers the last application route per selected node identity and restores it when reopening the window or restarting Electron. Explicit deep links take precedence; pairing URLs are never saved. See [the real-task acceptance record](../../docs/DESKTOP_ACCEPTANCE.md) for the validated task, cancellation, Git diff and restart scenarios.
