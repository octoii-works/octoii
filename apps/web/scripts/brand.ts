/**
 * Draws Octoii's mark and wordmark into `brand/` at the repository's root. The mark is Oii — a capital O and two i's —
 * drawn as pixel type on grids of 16, 32 and 64 cells, each grid for the sizes that are whole multiples of it: the O a
 * rounded rectangle whose corners are clean pixel arcs, its strokes a little heavier than the i's stems, as a curve must
 * be to look as heavy as a straight; the i's flat stems under their dots, the O closer to them than they are to each
 * other. The wordmark is "Octoii" in the same letters on the grid of 32. The app's icons are the wordmark, white on
 * #141414; the favicon, too small for the word, the mark on its grid of 16.
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

/** An i on one grid: its stem's width, the x-height, its dot's height and the gap over the stem, and the radii of the
 * stem's and the dot's corners. */
type LetterI = { readonly stem: number; readonly xHeight: number; readonly dot: number; readonly gap: number; readonly corners: { stem: number; dot: number } };

/** An i at `left`, standing on `base`. */
function letterI(left: number, base: number, { stem, xHeight, dot, gap, corners }: LetterI): Cells {
	const stemTop = base - xHeight + 1;
	return union(block({ left, top: stemTop, w: stem, h: xHeight, r: corners.stem }), block({ left, top: stemTop - gap - dot, w: stem, h: dot, r: corners.dot }));
}

/** The mark on one grid: the O's width, height, outer radius, side and top strokes and counter radius; the i's; the
 * O's gap to the first i and the gap between the i's; and how far the O reaches below the i's baseline. */
type Tier = {
	readonly o: { readonly w: number; readonly h: number; readonly r: number; readonly side: number; readonly top: number; readonly counter: number };
	readonly i: LetterI;
	readonly gaps: { readonly oi: number; readonly ii: number };
	readonly overshoot: number;
};

const TIERS: Record<16 | 32 | 64, Tier> = {
	16: {
		o: { w: 9, h: 10, r: 4, side: 2, top: 2, counter: 2 },
		i: { stem: 2, xHeight: 6, dot: 2, gap: 1, corners: { stem: 0, dot: 0 } },
		gaps: { oi: 1, ii: 1 },
		overshoot: 0,
	},
	32: {
		o: { w: 16, h: 18, r: 6, side: 3, top: 3, counter: 4 },
		i: { stem: 3, xHeight: 12, dot: 3, gap: 2, corners: { stem: 0, dot: 0 } },
		gaps: { oi: 2, ii: 2 },
		overshoot: 0,
	},
	64: {
		o: { w: 32, h: 37, r: 11, side: 6, top: 5, counter: 8 },
		i: { stem: 6, xHeight: 25, dot: 6, gap: 3, corners: { stem: 1, dot: 3 } },
		gaps: { oi: 4, ii: 5 },
		overshoot: 1,
	},
};

/** The mark on its grid of `n`, centred in it. */
function mark(n: number, { o, i, gaps, overshoot }: Tier): Cells {
	const left = Math.floor((n - (o.w + gaps.oi + i.stem + gaps.ii + i.stem)) / 2);
	const top = Math.floor((n - o.h) / 2);
	const base = top + o.h - 1 - overshoot;
	const first = left + o.w + gaps.oi;
	return union(ring({ left, top, w: o.w, h: o.h, r: o.r }, o, o.counter), letterI(first, base, i), letterI(first + i.stem + gaps.ii, base, i));
}

/** The wordmark: "Octoii" on the grid of 32, its O and i's the mark's, its o, c and t of the same strokes. */
function wordmark(): Cells {
	const { o, i } = TIERS[32];
	const [track, small, smallR, smallCounter] = [2, 11, 5, 2];
	const base = o.h - 1;
	const xTop = base - i.xHeight + 1;
	const s = i.stem;
	const stroke = { side: s, top: s };
	const letters: Cells[] = [ring({ left: 0, top: 0, w: o.w, h: o.h, r: o.r }, o, o.counter)];
	let x = o.w + track;
	// The c: an o opened on its right — its last column gone, so its terminals end flat, and its right half between them.
	const c = [...ring({ left: x, top: xTop, w: small, h: i.xHeight, r: smallR }, stroke, smallCounter)].filter((key) => {
		const [cx, cy] = point(key);
		return cx < x + small - 1 && !(cx > x + Math.floor(small / 2) && cy >= xTop + s && cy < xTop + i.xHeight - s);
	});
	letters.push(new Set(c));
	x += small - 1 + track;
	// The t: a stem rising 4 above the x-height, a bar of 8 across its top, its foot turned right by 2.
	const [rise, bar, foot] = [4, 8, 2];
	const stemLeft = x + Math.floor((bar - s) / 2);
	letters.push(
		block({ left: stemLeft, top: xTop - rise, w: s, h: base - xTop + rise + 1, r: 0 }),
		block({ left: x, top: xTop, w: bar, h: s, r: 0 }),
		block({ left: stemLeft + s, top: base - s + 1, w: foot, h: s, r: 0 }),
	);
	x += bar + track;
	letters.push(ring({ left: x, top: xTop, w: small, h: i.xHeight, r: smallR }, stroke, smallCounter));
	x += small + track;
	letters.push(letterI(x, base, i), letterI(x + s + track, base, i));
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

/** The wordmark as a square 1-bit icon of `size`: at the most whole pixels a cell that keep every cell within the
 * circle a mask may cut a maskable icon to, of radius 40%, centred. */
function icon(size: number): Buffer {
	const figure = trimmed(wordmark());
	const points = [...figure.cells].map(point);
	const reach = (k: number): number =>
		Math.max(
			...points.map(([x, y]) =>
				Math.hypot(
					Math.max(Math.abs(x - figure.w / 2), Math.abs(x + 1 - figure.w / 2)) * k,
					Math.max(Math.abs(y - figure.h / 2), Math.abs(y + 1 - figure.h / 2)) * k,
				),
			),
		);
	let k = 1;
	while (reach(k + 1) <= size * 0.4) k++;
	const [left, top] = [Math.floor((size - figure.w * k) / 2), Math.floor((size - figure.h * k) / 2)];
	const stride = Math.ceil(size / 8) + 1;
	const pixels = Buffer.alloc(stride * size);
	for (let y = 0; y < size; y++) {
		for (let x = 0; x < size; x++) {
			const [u, v] = [x - left, y - top];
			const inked = u >= 0 && v >= 0 && figure.cells.has(cell(Math.floor(u / k), Math.floor(v / k)));
			// Palette index 0 the ink, 1 the ground; each row led by its filter byte, 0.
			const at = y * stride + 1 + (x >> 3);
			if (!inked) pixels[at] = (pixels[at] ?? 0) | (0x80 >> (x & 7));
		}
	}
	const header = Buffer.alloc(13);
	header.writeUInt32BE(size, 0);
	header.writeUInt32BE(size, 4);
	header.set([1, 3, 0, 0, 0], 8);
	return Buffer.concat([
		Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
		chunk("IHDR", header),
		chunk("PLTE", Buffer.from(INK + GROUND, "hex")),
		chunk("IDAT", deflateSync(pixels, { level: 9 })),
		chunk("IEND", new Uint8Array()),
	]);
}

/** The favicon: the mark on the grid of 16, on its tile. */
function favicon(): string {
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" shape-rendering="crispEdges"><rect width="16" height="16" fill="#${GROUND}"/><path fill="#${INK}" d="${path(mark(16, TIERS[16]))}"/></svg>\n`;
}

if (import.meta.main) {
	await mkdir(BRAND, { recursive: true });
	const files: [string, string | Buffer][] = [
		["mark-16.svg", figureSvg(mark(16, TIERS[16]))],
		["mark-32.svg", figureSvg(mark(32, TIERS[32]))],
		["mark-64.svg", figureSvg(mark(64, TIERS[64]))],
		["wordmark.svg", figureSvg(wordmark())],
		["favicon.svg", favicon()],
		["icon-512.png", icon(512)],
		["icon-192.png", icon(192)],
		["apple-touch-icon.png", icon(180)],
	];
	for (const [name, data] of files) await writeFile(`${BRAND}${name}`, data);
	console.log(`Drew ${files.map(([name]) => name).join(", ")} into ${BRAND}`);
}
