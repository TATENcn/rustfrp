import { useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";

export function VersionPicker({ value, onChange, versions, disabled }: {
  value: string;
  onChange: (value: string) => void;
  versions: string[];
  disabled: boolean;
}) {
  const { text } = useI18n();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const list = useRef<HTMLDivElement>(null);
  const matches = versions.filter(version => version.includes(value.trim()));
  const choose = (version: string) => {
    onChange(version);
    setOpen(false);
    setActive(-1);
  };
  const move = (direction: number) => {
    setOpen(true);
    const next = Math.max(0, Math.min(matches.length - 1, active + direction));
    setActive(next);
    list.current?.children[next]?.scrollIntoView({ block: "nearest" });
  };
  return (
    <div className="relative min-w-0" onBlur={(event) => {
      if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
    }}>
      <Input
        id="version"
        role="combobox"
        autoComplete="off"
        aria-autocomplete="list"
        aria-expanded={open && !disabled}
        aria-controls="version-options"
        aria-activedescendant={open && active >= 0 ? `version-option-${active}` : undefined}
        value={value}
        onFocus={() => setOpen(true)}
        onClick={() => setOpen(true)}
        onChange={(event) => { onChange(event.target.value); setOpen(true); setActive(-1); }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            move(event.key === "ArrowDown" ? 1 : -1);
          } else if (event.key === "Enter" && open) {
            event.preventDefault();
            if (matches[active]) choose(matches[active]);
            else setOpen(false);
          } else if (event.key === "Escape" && open) {
            event.preventDefault();
            event.stopPropagation();
            setOpen(false);
          }
        }}
        placeholder={versions[0] ?? "0.70.1"}
        disabled={disabled}
      />
      {open && !disabled && (
        <div className="absolute top-full z-50 mt-1 w-full overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md">
          <div id="version-options" role="listbox" aria-label={text("可用版本", "Available versions")}
            ref={list} className="max-h-60 overflow-y-auto overscroll-contain p-1">
            {matches.map((version, index) => (
              <div key={version} id={`version-option-${index}`} role="option"
                aria-selected={value === version}
                className={`cursor-pointer rounded-sm px-3 py-2 text-sm ${active === index ? "bg-accent text-accent-foreground" : "hover:bg-accent"}`}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => choose(version)}>
                {version}
              </div>
            ))}
          </div>
          <p className="border-t px-3 py-2 text-xs text-muted-foreground">
            {matches.length ? text("输入版本号筛选，也可手动指定版本。", "Type to filter or enter a version manually.") : text("无匹配版本，将使用手动输入的版本号。", "No matches. Your entered version will be used.")}
          </p>
        </div>
      )}
    </div>
  );
}
