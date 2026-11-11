/**
 * The entry of the page a node serves. Opened at a pairing link, it first pairs this browser with the node. Then its
 * client of the node connects over the WebSocket, and the app runs on it.
 */
import { pageRoot, pairFromAddress, showApp, showPairing } from "./app/page.tsx";
import { sameOrigin } from "./app/reach.ts";

const root = pageRoot();
const failed = await pairFromAddress(sameOrigin);
if (failed === undefined) showApp(root, sameOrigin);
else showPairing(root, sameOrigin, failed);
