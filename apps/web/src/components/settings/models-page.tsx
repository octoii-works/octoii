/**
 * Models: where threads' models come from — the providers in use, each connected by its key, its sign-in, or what runs
 * it on this computer, and those to add — and how much of each plan's limits is used. The providers, and how those
 * whose standing matters stand, are read here once for both, again after each change, and as the window is focused
 * again, as the person comes back from a sign-in.
 */
import { type ReactNode, useState } from "react";
import { standingAsked } from "../../core/providers.ts";
import { useOnFocus } from "../effects/focus.ts";
import { useOutcome } from "../effects/read.ts";
import { useStatuses } from "../effects/statuses.ts";
import { Page } from "./layout.tsx";
import { ProvidersSection } from "./providers-section.tsx";
import { UsageSection } from "./usage-section.tsx";

/** The page. */
export function ModelsPage(): ReactNode {
	const [changes, setChanges] = useState(0);
	const changed = (): void => setChanges((count) => count + 1);
	useOnFocus(changed);
	const listed = useOutcome("providers.list", {}, String(changes));
	const providers = listed !== undefined && "result" in listed ? listed.result.providers : undefined;
	const statuses = useStatuses(providers?.filter(standingAsked), changes);
	return (
		<Page title="Models" description="Where your threads' models come from, and how much of each plan is used.">
			<ProvidersSection providers={providers} statuses={statuses} onChanged={changed} />
			<UsageSection providers={providers} statuses={statuses} onRefresh={changed} />
		</Page>
	);
}
