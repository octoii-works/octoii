/** What the tests build the node's answers of: a model as its catalogue gives it, and how a provider stands. */
import type { ElementFeedback, Limit, Model, ProviderStatus } from "@octoii-works/octoii-client";

/** A saved browser selection whose screenshot belongs to its source thread. */
export function capturedElement(fields: Partial<ElementFeedback> = {}): ElementFeedback {
	return {
		thread: "thread-A",
		page: "page-A",
		url: "https://example.com/form",
		document: "document-A",
		captured_at: 1_700_000_000_000,
		viewport: { width: 800, height: 600, scale: 2, scroll_x: 0, scroll_y: 120 },
		point: { x: 200, y: 180 },
		element: { tag: "button", role: "button", label: "Submit", text: "Submit form", selector: "#submit", rect: { x: 160, y: 160, width: 120, height: 40 } },
		screenshot: { ref: "sha256:captured", media_type: "image/png", filename: "selection.png" },
		...fields,
	};
}

/** The model `id` of `provider`, as the catalogue gives it, with `fields` laid over what a plain text model has. */
export function catalogued(provider: string, id: string, fields: Partial<Model> = {}): Model {
	return {
		id,
		provider,
		name: id,
		context_length: null,
		max_output_tokens: null,
		reads_images: false,
		reads_files: false,
		reasoning: null,
		parameters: null,
		pricing: null,
		release_date: null,
		searches_web: false,
		limits: { request_bytes: null, image_bytes: null, image_side_pixels: null, images: null },
		features: { instruction_updates: null, effort_updates: null, tool_choices: null },
		...fields,
	};
}

/** A provider that answers, signed in as `account` where it tells who, its plan's `limits` told. */
export function answersAs(account?: string, limits: readonly Limit[] = []): ProviderStatus {
	return { standing: { ...(account === undefined ? {} : { account }), limits: [...limits] } };
}

/** A provider that does not answer, `words` what the person does about it where it tells; one that tells nothing, where none. */
export function troubled(words?: string): ProviderStatus {
	return words === undefined ? {} : { standing: { limits: [], guidance: { words, commands: [] } } };
}
