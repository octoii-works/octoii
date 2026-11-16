/** Redeem the node's existing pairing protocol in the window's cookie store, before loading its page. */
import type { Session } from "electron";
import type { LocalNode } from "./local-node.ts";

export async function pairLocalNode(
	node: LocalNode,
	address: URL,
	session: { cookies: Pick<Session["cookies"], "get" | "flushStore">; fetch: Session["fetch"] },
): Promise<void> {
	const name = `octoii_session_${address.port || "80"}`;
	const cookies = await session.cookies.get({ url: address.origin, name });
	// Do not silently replace existing authorization, including one revoked by the user.
	if (cookies.length !== 0) return;
	const credential = await node.pairingCredential(address.origin);
	const response = await session.fetch(`${address.origin}/pair`, {
		method: "POST",
		headers: { "Content-Type": "application/json", Origin: address.origin },
		body: JSON.stringify({ credential }),
		credentials: "include",
		redirect: "error",
		signal: AbortSignal.timeout(5000),
	});
	if (!response.ok) throw new Error(`Local pairing failed (${response.status}).`);
	await response.arrayBuffer();
	if ((await session.cookies.get({ url: address.origin, name })).length === 0) throw new Error("The node did not establish a desktop session.");
	await session.cookies.flushStore();
}
