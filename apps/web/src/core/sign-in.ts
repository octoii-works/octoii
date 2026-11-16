/** The loopback callback may be pasted from another device that cannot reach the node. */
export function isSignInReturn(text: string): boolean {
	const given = text.trim();
	if (!URL.canParse(given)) return false;
	const address = new URL(given);
	return (
		address.protocol === "http:" &&
		address.hostname === "127.0.0.1" &&
		address.pathname === "/auth/callback" &&
		address.searchParams.has("state") &&
		(address.searchParams.has("code") || address.searchParams.has("error"))
	);
}
