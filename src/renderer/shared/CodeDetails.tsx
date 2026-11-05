export function CodeDetails({ children }: { children?: string | null }) {
  return (
    <pre className="min-w-0 overflow-auto whitespace-pre-wrap break-words font-mono text-sm [overflow-wrap:anywhere]">
      {children}
    </pre>
  );
}
