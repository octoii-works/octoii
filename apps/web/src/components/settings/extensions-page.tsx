/**
 * Settings › Extensions: what the machine was built with besides its core, each with a line of what it brings and a
 * switch. One switched off is told to new threads no more, and to a thread that works from its next compaction, so what
 * its model was told stays as it was till then; its views leave the page at once.
 */
import type { ReactNode } from "react";
import { useClient } from "@octoii-works/octoii-client/react";
import { useExtensions } from "../state/extensions.ts";
import { failed } from "../state/notices.ts";
import { Switch } from "../ui/switch.tsx";
import { Page, Row, Section } from "./layout.tsx";

/** The page. */
export function ExtensionsPage(): ReactNode {
	const client = useClient();
	const { listed, disabled } = useExtensions();
	const turn = (name: string, on: boolean): void => {
		const next = on ? disabled.filter((each) => each !== name) : [...disabled, name];
		client.setSettings({ environment: { disabled_extensions: next } }).catch((error: unknown) => failed("The extension could not be switched", error));
	};
	return (
		<Page title="Extensions" description="What this machine brings besides its core. All are on until you switch one off.">
			<Section
				title="Built in"
				description="A thread already working sees a change from its next compaction; a new thread, at once."
				anchor="setting-extensions"
			>
				{listed
					.toSorted((a, b) => a.title.localeCompare(b.title))
					.map((extension) => (
						<Row key={extension.name} title={extension.title} description={extension.description}>
							<Switch checked={!disabled.includes(extension.name)} onCheckedChange={(on) => turn(extension.name, on)} aria-label={`${extension.title} on`} />
						</Row>
					))}
			</Section>
		</Page>
	);
}
