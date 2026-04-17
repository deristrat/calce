interface IconProps {
  size?: number;
  className?: string;
}

const defaults = {
  xmlns: "http://www.w3.org/2000/svg",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export function IconDashboard({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <rect x="2" y="2" width="5" height="5" rx="1" />
      <rect x="9" y="2" width="5" height="5" rx="1" />
      <rect x="2" y="9" width="5" height="5" rx="1" />
      <rect x="9" y="9" width="5" height="5" rx="1" />
    </svg>
  );
}

export function IconBuilding({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <rect x="3" y="2" width="10" height="12" rx="1" />
      <line x1="6" y1="5" x2="6" y2="5.01" />
      <line x1="8" y1="5" x2="8" y2="5.01" />
      <line x1="10" y1="5" x2="10" y2="5.01" />
      <line x1="6" y1="8" x2="6" y2="8.01" />
      <line x1="8" y1="8" x2="8" y2="8.01" />
      <line x1="10" y1="8" x2="10" y2="8.01" />
      <rect x="6" y="11" width="4" height="3" />
    </svg>
  );
}

export function IconUsers({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <circle cx="6" cy="5" r="2" />
      <path d="M2 14c0-2.2 1.8-4 4-4s4 1.8 4 4" />
      <circle cx="11" cy="5" r="2" />
      <path d="M14 14c0-2.2-1.3-4-3-4" />
    </svg>
  );
}

export function IconChart({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="2 12 5 7 8 9 11 4 14 6" />
      <line x1="2" y1="14" x2="14" y2="14" />
    </svg>
  );
}

export function IconPalette({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <circle cx="8" cy="8" r="6" />
      <circle cx="6" cy="6" r="1" fill="currentColor" stroke="none" />
      <circle cx="9" cy="5" r="1" fill="currentColor" stroke="none" />
      <circle cx="11" cy="7" r="1" fill="currentColor" stroke="none" />
      <circle cx="5.5" cy="9" r="1" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function IconSearch({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <circle cx="7" cy="7" r="4" />
      <line x1="10" y1="10" x2="14" y2="14" />
    </svg>
  );
}

export function IconChevronRight({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="6 3 11 8 6 13" />
    </svg>
  );
}

export function IconChevronLeft({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="10 3 5 8 10 13" />
    </svg>
  );
}

export function IconSun({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <circle cx="8" cy="8" r="3" />
      <line x1="8" y1="1" x2="8" y2="3" />
      <line x1="8" y1="13" x2="8" y2="15" />
      <line x1="1" y1="8" x2="3" y2="8" />
      <line x1="13" y1="8" x2="15" y2="8" />
      <line x1="3.05" y1="3.05" x2="4.46" y2="4.46" />
      <line x1="11.54" y1="11.54" x2="12.95" y2="12.95" />
      <line x1="3.05" y1="12.95" x2="4.46" y2="11.54" />
      <line x1="11.54" y1="4.46" x2="12.95" y2="3.05" />
    </svg>
  );
}

export function IconMoon({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <path d="M13.5 8.5a5.5 5.5 0 1 1-6-6 4.5 4.5 0 0 0 6 6z" />
    </svg>
  );
}

export function IconLogout({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <path d="M6 14H3a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1h3" />
      <polyline points="10 11 14 8 10 5" />
      <line x1="14" y1="8" x2="6" y2="8" />
    </svg>
  );
}

export function IconArrowUp({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <line x1="8" y1="13" x2="8" y2="3" />
      <polyline points="4 7 8 3 12 7" />
    </svg>
  );
}

export function IconArrowDown({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <line x1="8" y1="3" x2="8" y2="13" />
      <polyline points="4 9 8 13 12 9" />
    </svg>
  );
}

export function IconX({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <line x1="4" y1="4" x2="12" y2="12" />
      <line x1="12" y1="4" x2="4" y2="12" />
    </svg>
  );
}

export function IconPlus({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <line x1="8" y1="3" x2="8" y2="13" />
      <line x1="3" y1="8" x2="13" y2="8" />
    </svg>
  );
}

export function IconSort({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="5 3 8 1 11 3" />
      <polyline points="5 13 8 15 11 13" />
    </svg>
  );
}

export function IconSortAsc({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="5 6 8 3 11 6" />
      <line x1="8" y1="3" x2="8" y2="13" />
    </svg>
  );
}

export function IconSortDesc({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="5 10 8 13 11 10" />
      <line x1="8" y1="3" x2="8" y2="13" />
    </svg>
  );
}

export function IconActivity({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <polyline points="1 8 4 8 6 3 8 13 10 6 12 8 15 8" />
    </svg>
  );
}

export function IconDatabase({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <ellipse cx="8" cy="4" rx="5" ry="2" />
      <path d="M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4" />
      <path d="M3 8c0 1.1 2.2 2 5 2s5-.9 5-2" />
    </svg>
  );
}

export function IconServer({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <rect x="2" y="3" width="12" height="4" rx="1" />
      <rect x="2" y="9" width="12" height="4" rx="1" />
      <line x1="5" y1="5" x2="5.01" y2="5" />
      <line x1="5" y1="11" x2="5.01" y2="11" />
    </svg>
  );
}

export function IconSettings({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <circle cx="8" cy="8" r="2.5" />
      <path d="M13 8a5 5 0 0 0-.1-1l1.3-1-1.5-2.6-1.6.5a5 5 0 0 0-1.7-1L9 1H7l-.4 1.9a5 5 0 0 0-1.7 1l-1.6-.5L1.8 6l1.3 1a5 5 0 0 0 0 2l-1.3 1 1.5 2.6 1.6-.5a5 5 0 0 0 1.7 1L7 15h2l.4-1.9a5 5 0 0 0 1.7-1l1.6.5 1.5-2.6-1.3-1a5 5 0 0 0 .1-1z" />
    </svg>
  );
}

export function IconCurrency({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} {...defaults}>
      <line x1="3" y1="5" x2="13" y2="5" />
      <line x1="3" y1="11" x2="13" y2="11" />
      <polyline points="10 2 6 8 10 14" />
    </svg>
  );
}
