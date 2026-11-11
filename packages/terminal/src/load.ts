/**
 * libghostty-vt for the page: its module fetched from the file the bundler emits beside the script, instantiated once
 * and shared by every terminal.
 */
// oxlint-disable-next-line typescript/triple-slash-reference -- the bundler's files have no module to import their type from
/// <reference path="./wasm.d.ts" />
import vtUrl from "../vendor/ghostty-vt.wasm";
import { Ghostty } from "./ghostty.ts";

let loading: Promise<Ghostty> | undefined;

/** The page's libghostty-vt, loaded on first use; a failed load is tried again on the next call. */
export function loadGhostty(): Promise<Ghostty> {
	loading ??= Ghostty.instantiate(fetch(new URL(vtUrl, import.meta.url))).catch((error: unknown) => {
		loading = undefined;
		throw error;
	});
	return loading;
}
