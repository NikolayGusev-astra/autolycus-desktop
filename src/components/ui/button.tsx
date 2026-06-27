export function Button({ className, variant = "default", size = "default", ...props }: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: string; size?: string }) {
  const base = "inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors";
  const variants: Record<string, string> = {
    default: "bg-primary text-primary-foreground hover:bg-primary/90",
    ghost: "hover:bg-accent hover:text-accent-foreground",
  };
  const sizes: Record<string, string> = { default: "h-9 px-3", icon: "h-9 w-9" };
  return (
    <button className={`${base} ${variants[variant] || ""} ${sizes[size] || ""} disabled:opacity-50 disabled:pointer-events-none ${className || ""}`} {...props} />
  );
}
