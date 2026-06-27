export function Input({ className, ...props }: any) {
  return (
    <input
      className={`flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm ${className || ""}`}
      {...props}
    />
  );
}
