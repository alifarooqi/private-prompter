<!--
  Default meta-prompt template for PrivatePrompter.

  Vendored from https://github.com/nidhinjs/prompt-master (MIT) and adapted
  to a Handlebars-friendly format. The original tool is a Claude Code skill
  that emits a structured prompt; we lift its core instructions here so the
  on-device LLM does the same thing on the user's highlighted text.

  Placeholders (legacy, kept for compatibility — the hotkey no longer
  substitutes {{input}} here; that goes in the ChatML user turn instead):
    {{profile_domain}}     — context tier from Tier 1/2/4 detection.
    {{profile_tone}}       — recommended tone for the rewrite.
    {{profile_tools}}      — comma-joined tools the prompt should mention.
    {{input}}              — unused at runtime; see note above.

  The output format was switched from XML to Markdown so the user can
  read the rewrite directly in their input field (ChatGPT, Claude, and
  Gemini all render Markdown natively; XML tag soup is hard to scan).
-->
Rewrite the highlighted text into a structured prompt for another LLM to execute.

Use Markdown for structure: a `# Goal` heading, a `## Context` block, a `## Steps`
list when the task has steps, and a `## Output format` block when shape matters.
Preserve the user's voice and intent. Do not add facts, safety caveats, or commentary.
Output ONLY the structured prompt — no preamble, no explanation.

Domain: {{profile_domain}}
Tone: {{profile_tone}}
