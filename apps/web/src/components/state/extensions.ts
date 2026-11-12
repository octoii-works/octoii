/**
 * The extensions of the machine the page reached, as its welcome lists them, and which the person switched off, as the
 * environment's settings say: what an extension brings is shown only while it is there and on.
 */
import type { ExtensionListed } from "@octoii-works/octoii-client";
import { useConnection, useSettings } from "@octoii-works/octoii-client/react";

/** The machine's extensions, and the names of those switched off. */
export type Extensions = { readonly listed: readonly ExtensionListed[]; readonly disabled: readonly string[] };

/** The extensions of the machine reached; none while it is not. */
export function useExtensions(): Extensions {
	const connection = useConnection();
	const disabled = useSettings()?.environment.disabled_extensions ?? [];
	return { listed: connection.status === "open" ? connection.extensions : [], disabled };
}

/** Whether the extension `name` is there and on. */
export function useExtension(name: string): boolean {
	const { listed, disabled } = useExtensions();
	return isOn({ listed, disabled }, name);
}

/** Whether the extension `name` is among `extensions` and on. */
export function isOn(extensions: Extensions, name: string): boolean {
	return extensions.listed.some((each) => each.name === name) && !extensions.disabled.includes(name);
}
