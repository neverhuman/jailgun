import type { ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeSanitize from 'rehype-sanitize';

function safeHttpUrl(value: string): string {
  if (!URL.canParse(value)) {
    return '';
  }
  const protocol = new URL(value).protocol;
  if (protocol === 'http:' || protocol === 'https:') {
    return value;
  }
  return '';
}
function ExternalLink({ href, children }: { href?: string; children?: ReactNode }) {
  if (href === undefined) {
    return <span>{children}</span>;
  }
  return <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>;
}

function ImageLabel({ alt }: { alt?: string }) {
  if (alt === undefined || alt.length === 0) {
    return <span>[Image]</span>;
  }
  return <span>[Image: {alt}]</span>;
}

export function Markdown({ text }: { text: string }) {
  return (
    <div className="conceptMarkdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSanitize]}
        urlTransform={safeHttpUrl}
        components={{
          a: ExternalLink,
          img: ImageLabel,
          h1: ({ children }) => <h3>{children}</h3>,
          h2: ({ children }) => <h3>{children}</h3>,
          table: ({ children }) => <div className="tableScroll"><table>{children}</table></div>,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
