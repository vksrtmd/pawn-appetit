import { Typography } from "@mantine/core";
import { memo } from "react";
import Markdown from "react-markdown";
import { commentRehypePlugins, isPlainText, normalizeTiptapMarkdown } from "@/utils/markdown";

function CommentInner({ comment }: { comment: string }) {
  // Big repertoires have thousands of comments, most of them without any formatting
  if (isPlainText(comment)) {
    return (
      <Typography style={{ display: "inline", lineHeight: "inherit" }}>
        <span>{comment.trim()}</span>
      </Typography>
    );
  }

  const normalized = normalizeTiptapMarkdown(comment);
  const multipleLine = normalized.split("\n").filter((v) => v.trim() !== "").length > 1;

  return (
    <Typography
      style={{
        display: multipleLine ? "block" : "inline",
        lineHeight: multipleLine ? undefined : "inherit",
      }}
    >
      <Markdown
        components={{
          a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer" />,
          p: ({ children }) => (
            <span
              style={{
                display: multipleLine ? "block" : "inline",
                marginBottom: multipleLine ? undefined : 0,
              }}
            >
              {children}
            </span>
          ),
        }}
        rehypePlugins={commentRehypePlugins}
      >
        {normalized}
      </Markdown>
    </Typography>
  );
}

export const Comment = memo(CommentInner);
