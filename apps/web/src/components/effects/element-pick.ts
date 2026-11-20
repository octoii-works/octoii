/** One user-initiated capture, cancelled by changing its page identity or leaving its view. */
import { useLayoutEffect, useRef, useState } from "react";
import type { ElementFeedback, PageListed } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { failed } from "../state/notices.ts";
import type { ElementSelection, PageInteraction } from "./page.ts";

type Selection = { readonly mode: "pick" | "capturing" } | { readonly mode: "annotate"; readonly feedback: ElementFeedback };

export function useElementPicker(
	thread: string,
	page: PageListed,
): PageInteraction & { readonly start: () => void; readonly feedback: ElementFeedback | undefined } {
	const client = useClient();
	const identity = `${thread}\n${page.id}\n${page.url}`;
	const [state, setState] = useState<(Selection & { readonly identity: string }) | undefined>(undefined);
	const selection = state?.identity === identity ? state : undefined;
	const mode = selection?.mode ?? "browse";
	const pending = useRef<object | undefined>(undefined);
	useLayoutEffect(
		() => () => {
			pending.current = undefined;
		},
		[identity],
	);
	const cancel = (): void => {
		pending.current = undefined;
		setState(undefined);
	};
	const select = (selection: ElementSelection): void => {
		if (pending.current !== undefined || mode !== "pick") return;
		const capture = {};
		pending.current = capture;
		setState({ identity, mode: "capturing" });
		void client.pickElement({ thread, page: page.id, ...selection }).then(
			(feedback) => {
				if (pending.current !== capture) return;
				pending.current = undefined;
				if (feedback.thread !== thread || feedback.page !== page.id) {
					setState({ identity, mode: "pick" });
					failed("The element could not be captured", new Error("The capture belongs to another page or thread."));
					return;
				}
				setState({ identity, mode: "annotate", feedback });
			},
			(error: unknown) => {
				if (pending.current !== capture) return;
				pending.current = undefined;
				setState({ identity, mode: "pick" });
				failed("The element could not be captured", error);
			},
		);
	};
	return {
		mode,
		select,
		cancel,
		start: () => setState({ identity, mode: "pick" }),
		feedback: selection?.mode === "annotate" ? selection.feedback : undefined,
	};
}
