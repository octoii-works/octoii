# Octoii desktop

A thin Electron shell around the Web interface served by a local Octoii node. macOS first; the shell also keeps the standard Windows / Linux window lifecycle.

From the repository root:

```sh
bun install
bun run desktop
```

The node must already be running at `http://127.0.0.1:7600`. To serve the current checkout's interface, build it and pass its directory to the node:

```sh
bun run --cwd apps/web build
octoii serve --app "$PWD/apps/web/dist"
```

Use the existing pairing screen, or pass a fresh link from `octoii pair` as `OCTOII_DESKTOP_URL`. This variable accepts only loopback HTTP URLs and can also select a different local port. Treat pairing links as credentials. Electron keeps its own session cookies, independently of the browser.

Closing the window or quitting Electron leaves the node and its tasks running. On macOS, the Dock reopens a closed window. Failed initial connections offer Retry / Close. External HTTP(S) links open in the system browser. The renderer is sandboxed, has no Node.js access or preload bridge, and cannot navigate to another origin.

This first shell does not install or start a node, bundle the Web assets, provide an installer, or implement native notifications. It displays whichever Web version the selected node serves. Phone work follows the desktop prototype.
