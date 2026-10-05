// Markdown made into an HTML document for `html-print.ts` to lay out. The
// stylesheet is the whole look of the PDF: a quiet, readable page in the
// bundled fonts, close to how GitHub shows a README.

import { Marked } from 'marked';

export type Typeface = 'sans' | 'serif';

const marked = new Marked({
	gfm: true,
	renderer: {
		// A form control draws nothing the layout can measure; a box can.
		checkbox({ checked }) {
			return `<span class="check${checked ? ' done' : ''}" aria-hidden="true"></span> `;
		}
	}
});

const stylesheet = (typeface: Typeface) => `
html { color: #1f2328; background: #fff; }
body {
	margin: 0;
	font-family: ${typeface === 'serif' ? 'serif' : 'sans-serif'};
	font-size: 15px;
	line-height: 1.6;
	overflow-wrap: break-word;
}
body > :first-child { margin-top: 0; }
p, ul, ol, blockquote, pre, table, dl, details { margin: 0 0 14px; }
h1, h2, h3, h4, h5, h6 { margin: 26px 0 12px; font-weight: 700; line-height: 1.25; }
h1 { font-size: 2em; padding-bottom: 0.3em; border-bottom: 1px solid #d1d9e0; }
h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid #d1d9e0; }
h3 { font-size: 1.25em; }
h4 { font-size: 1em; }
h5 { font-size: 0.875em; }
h6 { font-size: 0.85em; color: #59636e; }
a { color: #0969da; text-decoration: underline; }
strong { font-weight: 700; }
hr { height: 0; margin: 24px 0; border: 0; border-top: 2px solid #d1d9e0; }
ul, ol { padding-left: 2em; }
li + li, li > ul, li > ol { margin-top: 4px; }
li > ul, li > ol { margin-bottom: 0; }
li:has(> .check), li:has(> p > .check) { list-style: none; }
.check {
	display: inline-block;
	box-sizing: border-box;
	width: 0.85em;
	height: 0.85em;
	margin: 0 0.35em 0 -1.3em;
	vertical-align: -0.08em;
	border: 1.5px solid #8c959f;
	border-radius: 3px;
}
.check.done { background: #0969da; border-color: #0969da; }
blockquote { padding: 0 1em; color: #59636e; border-left: 4px solid #d1d9e0; }
blockquote > :last-child { margin-bottom: 0; }
code, kbd, samp, pre { font-family: monospace; }
code {
	padding: 0.15em 0.35em;
	font-size: 85%;
	background: #eff1f3;
	border-radius: 5px;
}
pre {
	padding: 14px 16px;
	font-size: 85%;
	line-height: 1.45;
	background: #f6f8fa;
	border-radius: 6px;
	white-space: pre-wrap;
	overflow-wrap: anywhere;
}
pre code { padding: 0; font-size: 100%; background: none; border-radius: 0; }
kbd {
	padding: 0.1em 0.4em;
	font-size: 85%;
	border: 1px solid #d1d9e0;
	border-bottom-width: 2px;
	border-radius: 5px;
}
mark { background: #fff8c5; color: inherit; }
table { border-collapse: collapse; border-spacing: 0; }
th, td { padding: 6px 13px; border: 1px solid #d1d9e0; }
th { font-weight: 700; background: #f6f8fa; }
tr:nth-child(2n) td { background: #f6f8fa; }
img { max-width: 100%; }
del { color: #59636e; }
dt { font-weight: 700; }
dd { margin: 0 0 8px 1.5em; }
`;

function escape(text: string) {
	return text.replace(
		/[&<>"]/g,
		(character) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[character]!
	);
}

/// A whole HTML document for `markdown`, titled by its first heading.
export function markdownDocument(markdown: string, typeface: Typeface) {
	const source = markdown.replace(/^\uFEFF/, '');
	const tokens = marked.lexer(source);
	const body = marked.parser(tokens);
	const heading = tokens.find((token) => token.type === 'heading');
	const title = heading && 'text' in heading ? heading.text.replace(/[*_`~[\]]/g, '') : '';
	return `<!doctype html><html><head><meta charset="utf-8"><title>${escape(title)}</title><style>${stylesheet(typeface)}</style></head><body>${body}</body></html>`;
}
