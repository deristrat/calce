import type { ReactNode } from "react";

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: ReactNode;
  disabled?: boolean;
}

function Toggle({ checked, onChange, label, disabled = false }: ToggleProps) {
  const button = (
    <button
      type="button"
      className={`ds-toggle${checked ? " ds-toggle--checked" : ""}`}
      onClick={() => onChange(!checked)}
      aria-pressed={checked}
      disabled={disabled}
    />
  );

  if (!label) return button;

  return (
    <label className="ds-kv-inline__item ds-cursor-pointer">
      {button}
      <span>{label}</span>
    </label>
  );
}

export default Toggle;
