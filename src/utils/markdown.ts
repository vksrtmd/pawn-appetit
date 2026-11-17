import type { Options as MarkdownOptions } from "react-markdown";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema, type Options as SanitizeSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";

// Markdown/GFM/HTML/tiptap syntax; chess notation like "Qh5+" or "1...e5" mid-text stays plain
const MARKDOWN_SYNTAX =
    /[*_`[\]<&\\~]|\+\+|[a-z][a-z0-9+.-]*:\/\/|www\.|@|^\s*([-+>#]|\d+[.)])|^\s{4}|\n/i;

// Text that Markdown would render unchanged, so it can skip the slow parser
export function isPlainText(text: string): boolean {
    return !MARKDOWN_SYNTAX.test(text);
}

// The annotation editor stores underline as ++text++
export function normalizeTiptapMarkdown(comment: string) {
    return comment.replace(/\+\+([\s\S]+?)\+\+/g, "<u>$1</u>");
}

// Comments come from PGN files and databases the user didn't write, so raw HTML in them
// is sanitized (GitHub's allowlist: no scripts, event handlers, iframes or javascript: URLs).
// <u> is added back because the editor's underline becomes <u>.
export const commentSanitizeSchema: SanitizeSchema = {
    ...defaultSchema,
    tagNames: [...(defaultSchema.tagNames ?? []), "u"],
};

export const commentRehypePlugins: NonNullable<MarkdownOptions["rehypePlugins"]> = [
    rehypeRaw,
    [rehypeSanitize, commentSanitizeSchema],
    remarkGfm,
];
