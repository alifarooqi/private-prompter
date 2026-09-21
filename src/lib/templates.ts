/**
 * Phase 7 typed bindings for templates. The user-editable surface is
 * intentionally small for Phase 1 — the Settings → Templates tab shows the
 * three bundled templates and lets the user preview what each one does.
 * Phase 7 (Polish) will add a Monaco editor and live test runner.
 */

export interface TemplateSummary {
  id: string;
  display_name: string;
  source: "builtin" | "user";
  description: string;
}

export const BUILTIN_TEMPLATES: TemplateSummary[] = [
  {
    id: "prompt-master",
    display_name: "Prompt Master (default)",
    source: "builtin",
    description:
      "Transforms raw thought into a structured prompt using XML tags. Adapted from nidhinjs/prompt-master (MIT).",
  },
  {
    id: "universal",
    display_name: "Universal",
    source: "builtin",
    description: "Tight rewrite for any text — no assumptions about domain.",
  },
  {
    id: "concise",
    display_name: "Concise",
    source: "builtin",
    description: "Shorten without losing meaning.",
  },
];