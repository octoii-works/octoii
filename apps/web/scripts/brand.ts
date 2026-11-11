/**
 * Draws Octoii's wordmark and mark into `brand/` at the repository's root. The wordmark is "Octoii" as bold, condensed
 * pixel type: tall narrow letters, the O and o rounded rectangles whose corners are short, clean pixel arcs, flat stems
 * of one stroke. The app's icons are the wordmark, white on #141414, filling 90% of their width, but for the one a mask
 * may cut to a circle. The mark, for the favicon, too small for the word, is Oii — the wordmark's first letter and its
 * last two — on a grid of 16.
 *
 *   bun scripts/brand.ts
 */
import { mkdir, writeFile } from "node:fs/promises";
import { crc32, deflateSync } from "node:zlib";

const BRAND = new URL("../../../brand/", import.meta.url).pathname;

/** The ink and the ground of the icons. */
const INK = "ffffff";
const GROUND = "141414";

/** A figure's cells, each `x,y`, x across and y down. */
type Cells = Set<string>;

const cell = (x: number, y: number): string => `${x},${y}`;

const point = (key: string): [number, number] => {
	const [x = 0, y = 0] = key.split(",").map(Number);
	return [x, y];
};

/** How far each of the first `rows` rows of a corner of radius `r` stands in from the side, row 0 the top edge: each
 * row's inset from the circle, rounded once, so the steps shorten across and lengthen down in order. */
function insets(r: number, rows: number): number[] {
	return Array.from({ length: rows }, (_, y) => {
		const dy = r - (y + 0.5);
		return dy <= 0 ? 0 : Math.round(r - Math.sqrt(Math.max(0, r * r - dy * dy)));
	});
}

/** A rectangle at `left`, `top` of `w` by `h` cells, its corners of radius `r`. */
type Box = { readonly left: number; readonly top: number; readonly w: number; readonly h: number; readonly r: number };

/** A rounded rectangle's rows, each its first and last column. */
function rounded({ left, top, w, h, r }: Box): Map<number, [number, number]> {
	const steps = insets(r, h);
	const rows = new Map<number, [number, number]>();
	for (let y = 0; y < h; y++) {
		const k = Math.min(y, h - 1 - y);
		const inset = k < r ? (steps[k] ?? 0) : 0;
		rows.set(top + y, [left + inset, left + w - 1 - inset]);
	}
	return rows;
}

/** A rectangle's cells, its corners rounded by the pixel arc of its radius; square where that is 0. */
function block(box: Box): Cells {
	const cells: Cells = new Set();
	for (const [y, [a, b]] of rounded(box)) for (let x = a; x <= b; x++) cells.add(cell(x, y));
	return cells;
}

/** A ring's strokes: at either side, and at top and bottom. */
type Stroke = { readonly side: number; readonly top: number };

/** A rounded ring: the rectangle `outer`, less its counter inside `stroke`, the counter's corners of radius `r`. */
function ring(outer: Box, stroke: Stroke, r: number): Cells {
	const counter = rounded({ left: outer.left + stroke.side, top: outer.top + stroke.top, w: outer.w - 2 * stroke.side, h: outer.h - 2 * stroke.top, r });
	const cells: Cells = new Set();
	for (const [y, [a, b]] of rounded(outer)) {
		const hole = counter.get(y);
		for (let x = a; x <= b; x++) if (hole === undefined || x < hole[0] || x > hole[1]) cells.add(cell(x, y));
	}
	return cells;
}

const union = (...sets: Cells[]): Cells => new Set(sets.flatMap((set) => [...set]));

/** An O: its width, height and outer radius, its side and top strokes, and its counter's radius. */
type LetterO = { readonly w: number; readonly h: number; readonly r: number; readonly side: number; readonly top: number; readonly counter: number };

/** An i: its stem's width, the x-height, and its square dot's side and the gap over the stem. */
type LetterI = { readonly stem: number; readonly xHeight: number; readonly dot: number; readonly gap: number };

/** An i at `left`, standing on `base`. */
function letterI(left: number, base: number, { stem, xHeight, dot, gap }: LetterI): Cells {
	const stemTop = base - xHeight + 1;
	return union(block({ left, top: stemTop, w: stem, h: xHeight, r: 0 }), block({ left, top: stemTop - gap - dot, w: stem, h: dot, r: 0 }));
}

/** The mark, Oii, on the grid of 16: its O, its i's, and the gaps from the O to the first i and between the i's. */
const MARK = {
	o: { w: 7, h: 12, r: 2, side: 2, top: 2, counter: 1 },
	i: { stem: 2, xHeight: 8, dot: 2, gap: 1 },
	gaps: { oi: 1, ii: 1 },
} as const satisfies { o: LetterO; i: LetterI; gaps: { oi: number; ii: number } };

/** The grid the mark is drawn on. */
const GRID = 16;

/** The mark, centred on its grid. */
function mark(): Cells {
	const { o, i, gaps } = MARK;
	const left = Math.floor((GRID - (o.w + gaps.oi + i.stem + gaps.ii + i.stem)) / 2);
	const top = Math.floor((GRID - o.h) / 2);
	const base = top + o.h - 1;
	const first = left + o.w + gaps.oi;
	return union(ring({ left, top, w: o.w, h: o.h, r: o.r }, o, o.counter), letterI(first, base, i), letterI(first + i.stem + gaps.ii, base, i));
}

/** The wordmark's letters: its O and i's; its lowercase o's width, radius and counter's radius, the c an o opened; its
 * t's rise above the x-height, bar and foot; and the space after each letter but the last, O to i. */
type Word = {
	readonly o: LetterO;
	readonly i: LetterI;
	readonly small: { readonly w: number; readonly r: number; readonly counter: number };
	readonly t: { readonly rise: number; readonly bar: number; readonly foot: number };
	readonly tracks: readonly [number, number, number, number, number];
};

/** The wordmark's letters: capitals 18 cells high and narrow, the x-height 13, bold — stems of 3 — so the word stands
 * near square; the t, open at its sides, set close to the o. */
const WORD: Word = {
	o: { w: 11, h: 18, r: 4, side: 3, top: 3, counter: 2 },
	i: { stem: 3, xHeight: 13, dot: 3, gap: 1 },
	small: { w: 9, r: 3, counter: 1 },
	t: { rise: 4, bar: 6, foot: 2 },
	tracks: [2, 2, 1, 2, 2],
};

/** The wordmark, "Octoii": its o, c and t of the same strokes as its O and i's. */
function wordmark(): Cells {
	const { o, i, small, t, tracks } = WORD;
	const [oc, ct, to, oi, ii] = tracks;
	const base = o.h - 1;
	const xTop = base - i.xHeight + 1;
	const s = i.stem;
	const stroke = { side: s, top: s };
	const lower = { top: xTop, w: small.w, h: i.xHeight, r: small.r };
	const letters: Cells[] = [ring({ left: 0, top: 0, w: o.w, h: o.h, r: o.r }, o, o.counter)];
	let x = o.w + oc;
	// The c: an o opened on its right — its last column gone, so its terminals end flat, and its right half between them.
	const c = [...ring({ left: x, ...lower }, stroke, small.counter)].filter((key) => {
		const [cx, cy] = point(key);
		return cx < x + small.w - 1 && !(cx > x + Math.floor(small.w / 2) && cy >= xTop + s && cy < xTop + i.xHeight - s);
	});
	letters.push(new Set(c));
	x += small.w - 1 + ct;
	// The t: a stem rising above the x-height, a bar across its top, its foot turned right.
	const stemLeft = x + Math.floor((t.bar - s) / 2);
	letters.push(
		block({ left: stemLeft, top: xTop - t.rise, w: s, h: base - xTop + t.rise + 1, r: 0 }),
		block({ left: x, top: xTop, w: t.bar, h: s, r: 0 }),
		block({ left: stemLeft + s, top: base - s + 1, w: t.foot, h: s, r: 0 }),
	);
	x += t.bar + to;
	letters.push(ring({ left: x, ...lower }, stroke, small.counter));
	x += small.w + oi;
	letters.push(letterI(x, base, i), letterI(x + s + ii, base, i));
	return union(...letters);
}

/** `cells` moved to start at 0, 0, with their width and height. */
function trimmed(cells: Cells): { cells: Cells; w: number; h: number } {
	const points = [...cells].map(point);
	const x0 = Math.min(...points.map(([x]) => x));
	const y0 = Math.min(...points.map(([, y]) => y));
	const moved = points.map(([x, y]) => [x - x0, y - y0] as const);
	return {
		cells: new Set(moved.map(([x, y]) => cell(x, y))),
		w: Math.max(...moved.map(([x]) => x)) + 1,
		h: Math.max(...moved.map(([, y]) => y)) + 1,
	};
}

/** `cells` as an SVG path of rectangles: each row's runs, carried down while the rows below run the same. */
function path(cells: Cells): string {
	const runs = new Map<number, string[]>();
	const rows = Map.groupBy([...cells].map(point), ([, y]) => y);
	for (const [y, row] of rows) {
		const xs = row.map(([x]) => x).toSorted((a, b) => a - b);
		let start = xs[0] ?? 0;
		xs.forEach((x, index) => {
			if (xs[index + 1] === x + 1) return;
			runs.set(y, [...(runs.get(y) ?? []), `${start} ${x + 1}`]);
			start = xs[index + 1] ?? 0;
		});
	}
	const drawn = new Set<string>();
	const parts: string[] = [];
	for (const y of [...runs.keys()].toSorted((a, b) => a - b)) {
		for (const run of runs.get(y) ?? []) {
			if (drawn.has(`${y} ${run}`)) continue;
			let h = 1;
			while (runs.get(y + h)?.includes(run)) drawn.add(`${y + h++} ${run}`);
			const [a = 0, b = 0] = run.split(" ").map(Number);
			parts.push(`M${a} ${y}h${b - a}v${h}h-${b - a}z`);
		}
	}
	return parts.join("");
}

/** `cells` alone as an SVG in the colour of the text around it. */
function figureSvg(cells: Cells): string {
	const figure = trimmed(cells);
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${figure.w} ${figure.h}" shape-rendering="crispEdges"><path fill="currentColor" d="${path(figure.cells)}"/></svg>\n`;
}

/** A PNG chunk of `kind`, its length before and its checksum after. */
function chunk(kind: string, body: Uint8Array): Buffer {
	const head = Buffer.alloc(8);
	head.writeUInt32BE(body.length);
	head.write(kind, 4, "latin1");
	const tail = Buffer.alloc(4);
	tail.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), body])));
	return Buffer.concat([head, body, tail]);
}

/** How much of the span from `a` to `b` lies within that from `c` to `d`. */
const overlap = (a: number, b: number, c: number, d: number): number => Math.max(0, Math.min(b, d) - Math.max(a, c));

/** How much of the pixel at `x`, `y` the figure's cells cover, each cell `k` pixels a side, the figure's corner at
 * `left`, `top`: 0 to 1, a whole pixel inked or bare wherever its edges fall on whole pixels. */
function coverage(cells: Cells, k: number, [left, top]: [number, number], [x, y]: [number, number]): number {
	let covered = 0;
	for (let cx = Math.floor((x - left) / k); cx * k + left < x + 1; cx++) {
		for (let cy = Math.floor((y - top) / k); cy * k + top < y + 1; cy++) {
			if (cells.has(cell(cx, cy))) covered += overlap(x, x + 1, left + cx * k, left + (cx + 1) * k) * overlap(y, y + 1, top + cy * k, top + (cy + 1) * k);
		}
	}
	return covered;
}

/** The wordmark as a square grey icon of `size`, white on #141414, at `k` pixels a cell, centred. */
function icon(size: number, k: number): Buffer {
	const figure = trimmed(wordmark());
	const corner: [number, number] = [Math.floor((size - figure.w * k) / 2), Math.floor((size - figure.h * k) / 2)];
	const [ink, ground] = [Number.parseInt(INK.slice(0, 2), 16), Number.parseInt(GROUND.slice(0, 2), 16)];
	const stride = size + 1;
	const pixels = Buffer.alloc(stride * size);
	for (let y = 0; y < size; y++) {
		// Each row led by its filter byte, 0.
		for (let x = 0; x < size; x++) pixels[y * stride + 1 + x] = Math.round(ground + (ink - ground) * coverage(figure.cells, k, corner, [x, y]));
	}
	const header = Buffer.alloc(13);
	header.writeUInt32BE(size, 0);
	header.writeUInt32BE(size, 4);
	header.set([8, 0, 0, 0, 0], 8);
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk("IHDR", header),
		chunk("IDAT", deflateSync(pixels, { level: 9 })),
		chunk("IEND", new Uint8Array()),
	]);
}

/** The pixels a cell at which the wordmark fills `share` of an icon's width: an icon nothing cuts but its corners. */
function filling(size: number, share: number): number {
	return (size * share) / trimmed(wordmark()).w;
}

/** The pixels a cell at which the wordmark's farthest corner meets the circle a mask may cut an icon of `size` to, of
 * radius 40%. */
function maskable(size: number): number {
	const figure = trimmed(wordmark());
	const farthest = Math.max(
		...[...figure.cells]
			.map(point)
			.map(([x, y]) =>
				Math.hypot(Math.max(Math.abs(x - figure.w / 2), Math.abs(x + 1 - figure.w / 2)), Math.max(Math.abs(y - figure.h / 2), Math.abs(y + 1 - figure.h / 2))),
			),
	);
	return (size * 0.4) / farthest;
}

/** How much of an icon's width the wordmark fills where only the icon's corners are cut. */
const FILL = 0.9;

/** The favicon: the mark on its tile of 16. */
function favicon(): string {
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" shape-rendering="crispEdges"><rect width="16" height="16" fill="#${GROUND}"/><path fill="#${INK}" d="${path(mark())}"/></svg>\n`;
}

if (import.meta.main) {
	await mkdir(BRAND, { recursive: true });
	const files: [string, string | Buffer][] = [
		["mark.svg", figureSvg(mark())],
		["wordmark.svg", figureSvg(wordmark())],
		["favicon.svg", favicon()],
		["icon-512.png", icon(512, filling(512, FILL))],
		["icon-192.png", icon(192, filling(192, FILL))],
		["apple-touch-icon.png", icon(180, filling(180, FILL))],
		["icon-maskable-512.png", icon(512, maskable(512))],
	];
	for (const [name, data] of files) await writeFile(`${BRAND}${name}`, data);
	console.log(`Drew ${files.map(([name]) => name).join(", ")} into ${BRAND}`);
}
