import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { Components } from 'react-markdown'

// Flat-dark markdown for assistant output. Inline code is a chip; fenced code
// sits in a bordered block (the `.md pre code` reset in index.css neutralizes
// the chip styling inside blocks). Streams gracefully - partial markdown just
// resolves as more text arrives.
const components: Components = {
  p: ({ children }) => <p className="mb-2 leading-relaxed last:mb-0">{children}</p>,
  a: ({ children, href }) => (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="text-accent-ink underline underline-offset-2 hover:text-accent-2"
    >
      {children}
    </a>
  ),
  ul: ({ children }) => <ul className="mb-2 list-disc space-y-1 pl-5 last:mb-0">{children}</ul>,
  ol: ({ children }) => <ol className="mb-2 list-decimal space-y-1 pl-5 last:mb-0">{children}</ol>,
  li: ({ children }) => <li className="leading-relaxed">{children}</li>,
  h1: ({ children }) => <h1 className="mb-2 mt-3 text-[15px] font-semibold first:mt-0">{children}</h1>,
  h2: ({ children }) => <h2 className="mb-2 mt-3 text-[14.5px] font-semibold first:mt-0">{children}</h2>,
  h3: ({ children }) => <h3 className="mb-1.5 mt-2.5 text-[14px] font-semibold first:mt-0">{children}</h3>,
  strong: ({ children }) => <strong className="font-semibold text-ink">{children}</strong>,
  blockquote: ({ children }) => (
    <blockquote className="my-2 border-l-2 border-line-3 pl-3 text-ink-3">{children}</blockquote>
  ),
  hr: () => <hr className="my-3 border-line" />,
  code: ({ children, className }) => (
    <code className={`rounded bg-surface-2 px-1 py-0.5 font-mono text-[0.9em] ${className ?? ''}`}>{children}</code>
  ),
  pre: ({ children }) => (
    <pre className="my-2 overflow-x-auto rounded-lg border border-line bg-desktop/50 p-3 font-mono text-[12.5px] leading-relaxed">
      {children}
    </pre>
  ),
}

export function Markdown({ children }: { children: string }) {
  return (
    <div className="md text-[16.5px] leading-relaxed break-words text-ink-2">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {children}
      </ReactMarkdown>
    </div>
  )
}
