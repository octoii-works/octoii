/** Files a tool delivered to the person, saved in the thread by their original bytes, separately from model inputs. */
import type { InputContent, JsonValue } from "@octoii-works/octoii-client";
import { titleOf } from "./calls.ts";

/** A delivered file, with the tool result's optional display title. Its filename remains the file's own name. */
export type PresentedFile = Extract<InputContent, { type: "resource_link" }> & { readonly title: string | undefined };

/** Only explicit resource links deliver files; an unsuccessful execute may still hold files delivered before its failure. */
export function presentedFilesOf(content: readonly InputContent[], metadata: JsonValue | undefined): readonly PresentedFile[] {
	const title = titleOf(metadata);
	return content.flatMap((part) => (part.type === "resource_link" ? [{ ...part, title }] : []));
}
