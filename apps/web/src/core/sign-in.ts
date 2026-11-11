/**
 * A provider's sign-in in a browser, as the page sees it: the node's own address the issuer sends the browser back to,
 * `/sign-in/<provider>/callback`, where the node finishes the sign-in itself — or, from a browser of another device,
 * which cannot reach it, the address the person copies from its address bar and pastes, which the page finishes it from.
 */

/** The path a provider's sign-in sends the browser back to, its provider's id within it. */
const SIGN_IN_PATH = /^\/sign-in\/([^/]+)\/callback$/;

/** The provider a sign-in came back to `text` is of, where it is an address a sign-in sent a browser back to, with what the issuer gave it. */
export function signInReturnOf(text: string): string | undefined {
	const given = text.trim();
	if (!URL.canParse(given)) return undefined;
	const address = new URL(given);
	const provider = SIGN_IN_PATH.exec(address.pathname)?.[1];
	return provider === undefined || address.search.length <= 1 ? undefined : decodeURIComponent(provider);
}

/** Whether `text` is an address a sign-in sent a browser back to. */
export function isSignInReturn(text: string): boolean {
	return signInReturnOf(text) !== undefined;
}
