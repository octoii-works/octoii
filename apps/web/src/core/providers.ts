/**
 * The providers as settings show them: those in use — an account the person added, one holding a key or a sign-in, or
 * one reached through an agent or run on this computer that answers — each account right after the provider it is
 * another of; how each stands, in a line; and the catalogue a provider is added from, by how each connects.
 */
import type { Connects, ProviderShown as Provider, ProviderStatus } from "@octoii-works/octoii-client";

/**
 * Whether `provider` answers as `status` tells: signed in, where its driver tells how its account stands — with nothing
 * left for the person to do — else holding a credential; none while it is asked.
 */
export function connected(provider: Provider, status: ProviderStatus | undefined): boolean | undefined {
	if (status === undefined) return undefined;
	if (status.standing !== undefined) return status.standing.guidance === undefined;
	return provider.credential !== undefined && provider.credential !== "none";
}

/** How a provider stands where the node would not tell: it does not answer, `words` what is wrong. */
export function refusedStatus(words: string): ProviderStatus {
	return { standing: { limits: [], guidance: { words, commands: [] } } };
}

/** Whether `provider` is in use: an account the person added, one that holds a key or a sign-in, or one that answers as `status` tells. */
export function inUse(provider: Provider, status: ProviderStatus | undefined): boolean {
	if (provider.account !== undefined || (provider.credential !== undefined && provider.credential !== "none")) return true;
	return (provider.connects === "agent" || provider.connects === "server") && connected(provider, status) === true;
}

/**
 * Whether how `provider` stands is worth asking: an account the person added, one that holds a key or a sign-in, or one
 * reached through an agent or run on this computer, whose standing tells whether it is in use. One that holds nothing
 * is not in use, whatever it would say.
 */
export function standingAsked(provider: Provider): boolean {
	return (
		provider.account !== undefined ||
		(provider.credential !== undefined && provider.credential !== "none") ||
		provider.connects === "agent" ||
		provider.connects === "server"
	);
}

/** The providers of `providers` in use, as `statuses` tell, in the node's order, each account the person added right after the provider it is another of. */
export function inUseOf(providers: readonly Provider[], statuses: ReadonlyMap<string, ProviderStatus> | undefined): readonly Provider[] {
	const used = providers.filter((each) => inUse(each, statuses?.get(each.id)));
	const accounts = used.filter((each) => each.account !== undefined);
	const hosts = used.filter((each) => each.account === undefined);
	const orphans = accounts.filter((account) => !hosts.some((host) => host.id === account.account?.provider));
	return [...hosts.flatMap((host) => [host, ...accounts.filter((account) => account.account?.provider === host.id)]), ...orphans];
}

/** How a provider that answers stands, by how it is connected, where it says nothing of who it is connected as. */
const CONNECTED: Readonly<Record<Connects, string>> = { api_key: "API key", sign_in: "Signed in", agent: "Signed in", server: "Running" };

/** How a provider that does not answer stands, by how it is connected, where it says nothing of what is wrong. */
const NOT_CONNECTED: Readonly<Record<Connects, string>> = { api_key: "No API key", sign_in: "Not signed in", agent: "Not signed in", server: "Not running" };

/** How `provider` stands, in a line, as `status` tells: who it is connected as, or what is wrong; that it is asked, till it answers. */
export function standingOf(provider: Provider, status: ProviderStatus | undefined): string {
	if (status === undefined) return "Checking…";
	if (connected(provider, status) === true) return status.standing?.account ?? CONNECTED[provider.connects];
	const guidance = status.standing?.guidance;
	if (guidance === undefined) return NOT_CONNECTED[provider.connects];
	return [guidance.words, ...guidance.commands.map((command) => `\`${command}\``)].join(" ");
}

/** How `provider` stands in a few words, as the list tells it: who it is connected as, or only that it is not — what is wrong said in its details. */
export function standingShortOf(provider: Provider, status: ProviderStatus | undefined): string {
	if (status === undefined) return "Checking…";
	return connected(provider, status) === true ? (status.standing?.account ?? CONNECTED[provider.connects]) : NOT_CONNECTED[provider.connects];
}

/** Whether `status` tells that `provider`, in use, does not answer. */
export function troubled(provider: Provider, status: ProviderStatus | undefined): boolean {
	return connected(provider, status) === false;
}

/** A group of the catalogue: its title, and its providers. */
export type CatalogueGroup = { readonly title: string; readonly providers: readonly Provider[] };

/** The catalogue a provider is added from: the node's own providers — no account the person added — those signed in to, those reached by a key, and those run on this computer, each in the node's order. */
export function catalogueOf(providers: readonly Provider[]): readonly CatalogueGroup[] {
	const hosts = providers.filter((each) => each.account === undefined);
	const groups: readonly CatalogueGroup[] = [
		{ title: "Sign in", providers: hosts.filter((each) => each.connects === "sign_in" || each.connects === "agent") },
		{ title: "API key", providers: hosts.filter((each) => each.connects === "api_key") },
		{ title: "On this computer", providers: hosts.filter((each) => each.connects === "server") },
	];
	return groups.filter((group) => group.providers.length > 0);
}

/** Whether another account of `provider` may be added: any but a server on this computer, another account of which would reach the same server. */
export function takesAccounts(provider: Provider): boolean {
	return provider.account === undefined && provider.connects !== "server";
}
