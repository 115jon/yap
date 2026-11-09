/**
 * The console's theme, kept by next-themes in this browser (the same six
 * colourways as Yap; the console has no account settings to sync).
 */

import { useTheme } from "next-themes";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const THEMES = [
  { id: "system", label: "Match system" },
  { id: "dark", label: "Midnight" },
  { id: "light", label: "Light" },
  { id: "miku-dark", label: "Miku Dark" },
  { id: "miku-light", label: "Miku Light" },
  { id: "spiderman-dark", label: "Spider-Man Dark" },
  { id: "spiderman-light", label: "Spider-Man Light" },
] as const;

export function ThemePicker() {
  const { theme, setTheme } = useTheme();
  return (
    <Select value={theme ?? "system"} onValueChange={setTheme}>
      <SelectTrigger size="sm" aria-label="Theme" className="mb-1 w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {THEMES.map((option) => (
          <SelectItem key={option.id} value={option.id}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
