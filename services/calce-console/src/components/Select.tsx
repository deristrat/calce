import type { SelectHTMLAttributes } from "react";

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  error?: boolean;
}

function Select({ error = false, className, children, ...props }: SelectProps) {
  const classes = ["ds-select", error && "ds-select--error", className]
    .filter(Boolean)
    .join(" ");

  return (
    <select className={classes} {...props}>
      {children}
    </select>
  );
}

export default Select;
