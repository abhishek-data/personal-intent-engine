---
name: PIE — Personal Intent Engine
description: A reference book printed on black stock — ink ground, bone ink, one proofreader's red.
colors:
  ink: "#0F0E0C"
  ink-2: "#16130F"
  ink-3: "#1D1915"
  rule: "#2E2A26"
  rule-strong: "#4A433B"
  bone: "#EDE7DC"
  bone-2: "#B8AE9C"
  bone-3: "#8B8172"
  bone-4: "#6E6558"
  proof: "#E04A30"
  proof-deep: "#B8351E"
typography:
  display:
    fontFamily: "Spectral, 'Iowan Old Style', Georgia, serif"
    fontSize: "21px"
    fontWeight: 300
    lineHeight: 1
    letterSpacing: "0.01em"
  headline:
    fontFamily: "Spectral, 'Iowan Old Style', Georgia, serif"
    fontSize: "20px"
    fontWeight: 400
    lineHeight: 1.4
  title:
    fontFamily: "Spectral, 'Iowan Old Style', Georgia, serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
  body:
    fontFamily: "'Libre Franklin', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "'Libre Franklin', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "10px"
    fontWeight: 600
    letterSpacing: "0.12em"
  caption:
    fontFamily: "'Libre Franklin', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
  mono:
    fontFamily: "'Spline Sans Mono', ui-monospace, 'SF Mono', 'Cascadia Code', monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.65
rounded:
  none: "0"
  circle: "50%"
spacing:
  s1: "4px"
  s2: "8px"
  s3: "12px"
  s4: "16px"
  s5: "24px"
  s6: "32px"
  s7: "48px"
components:
  button-primary:
    backgroundColor: "{colors.bone}"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "10px 18px"
    typography: "{typography.label}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.bone-2}"
    rounded: "{rounded.none}"
    padding: "10px 18px"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.proof}"
    rounded: "{rounded.none}"
    padding: "10px 18px"
  text-button:
    backgroundColor: "transparent"
    textColor: "{colors.bone-3}"
    rounded: "{rounded.none}"
    padding: "2px 0"
    typography: "{typography.label}"
  input:
    backgroundColor: "transparent"
    textColor: "{colors.bone}"
    rounded: "{rounded.none}"
    padding: "8px 0"
    height: "38px"
  thumb-tab:
    backgroundColor: "transparent"
    textColor: "{colors.bone-3}"
    rounded: "{rounded.none}"
    padding: "4px 0 5px"
    typography: "{typography.label}"
  thumb-tab-active:
    backgroundColor: "transparent"
    textColor: "{colors.bone}"
  keycap:
    backgroundColor: "{colors.ink-2}"
    textColor: "{colors.bone}"
    rounded: "{rounded.none}"
    padding: "0 6px"
    height: "24px"
  record-button:
    backgroundColor: "transparent"
    rounded: "{rounded.circle}"
    width: "60px"
    height: "60px"
---

# Design System: PIE — Personal Intent Engine

## Overview

**Creative North Star: "The Personal Lexicon"**

PIE's correction store is literally a dictionary — `heard → canonical`, taught once and kept forever. So the interface is a reference book, and because the people using it work in dark editors at night, it is a reference book printed on black stock: a warm ink ground, bone-white ink, and one proofreader's red, the mark an editor makes on a proof. That last equivalence is the whole system. PIE marks up your speech the way a copy editor marks up a galley, so the accent colour is a mark, never a surface.

The system has no cards, no shadows in the main window, and no rounded corners. Structure is carried by ruled heads, hanging rhythm, hairlines, and a genuine type hierarchy running from 10px labels to 21px display. Two typefaces divide the work along the product's own seam: **Spectral** sets human speech — your transcript, the objective PIE took from it, your dictionary entries — and **Spline Sans Mono** sets machine output — the optimized prompt, the model's reply, key legends. **Libre Franklin** carries the chrome. You can read what PIE thinks a thing *is* from the face it is set in.

This replaces the previous system wholesale. That system was near-black with a single indigo accent applied to every interactive element, 12px radii, gradient buttons, a 2.5%-white card sheen, and a blurred glass overlay that shared no values with the main window. It was disciplined and completely generic — the category default. Nothing from its palette, type, density, chrome, or elevation model survives. What did survive is its structural discipline: complete tokenisation, one global `:focus-visible`, a `prefers-reduced-motion` neutraliser, and the rule that every correction stays visible.

**Key Characteristics:**
- Warm ink ground (`#0F0E0C`), never blue-black; dark-only by decision, not omission
- One accent, used only as marks and rules — under 2% of any screen
- Zero radius everywhere; exactly one circle in the entire application
- Zero shadows in the main window; depth is rules and tonal shift
- Serif for human speech, mono for machine output, grotesque for chrome
- Ruled heads instead of cards; ruled lines instead of boxed inputs
- Motion is ink, not screen: strokes strike in, nothing blinks or glows

## Colors

A warm, slightly brown-shifted ink stack carrying bone-white text and one vermilion mark. Restrained strategy: neutrals plus a single accent.

### Primary
- **Proof Red** (`#E04A30`): the editor's mark. It appears on the corrected term in a result, the arrow between heard and canonical, the active thumb tab's underline, the record mark, the focused field's rule, the checked checkbox, the download fill, and error labels. It is never a button fill, never a glow, never a background. Chosen at 4.8:1 on the ink ground so it stays legible at 10px.
- **Proof Deep** (`#B8351E`): the border of a destructive action at rest, and the pressed state of the mark.

### Neutral
- **Ink** (`#0F0E0C`): the page. Warm, not blue.
- **Ink Well** (`#16130F`): key legends and inline code — the only recessed surfaces left in the system.
- **Ink Pressed** (`#1D1915`): the active state of a ghost button.
- **Rule** (`#2E2A26`): every divider, every leaf head's hairline, every list separator.
- **Rule Strong** (`#4A433B`): the underline of an editable field, the edge of a key legend, the resting border of the record circle.
- **Bone** (`#EDE7DC`, 16.0:1): all primary text, and the fill of the affirmative button.
- **Bone Secondary** (`#B8AE9C`, 9.0:1): section labels, ghost-button text, the leading word of a usage line.
- **Bone Faint** (`#8B8172`, 5.2:1): captions, notes, timestamps, placeholders, the italic *heard* form of a correction.
- **Bone Disabled** (`#6E6558`, 3.4:1): disabled text and non-text marks only.

### Named Rules

**The Mark, Not the Field Rule.** Proof red is applied only to marks and rules — never as a surface fill, a glow, a card tint, or a button background. If a red element is larger than a rule or a small run of type, it is wrong. The audit test: screenshot any screen and the red should be hard to measure.

**The Contrast Floor Rule.** Every bone value used for text meets 4.5:1 on the ink ground; the number is recorded beside the token in `tokens.css`. `--bone-4` is below the floor and is reserved for disabled states and non-text marks. Never set body copy in it.

**The Two-Signal Rule.** State is never colour alone. Recording is a filled square *and* the word; transcribing is a hatched rule *and* the word; an error is a red rule *and* the word ERROR. This matters because the overlay must be readable at a glance over an arbitrary application.

## Typography

**Display / Entry Font:** Spectral (Production Type, SIL OFL 1.1)
**Chrome Font:** Libre Franklin (Impallari Type, SIL OFL 1.1)
**Machine Font:** Spline Sans Mono (Sorkin/Velimirovic, SIL OFL 1.1)

All three are vendored into `ui/src/fonts/` with the OFL text, because the app's CSP forbids remote font origins. 27 subsets, ~334KB on disk.

**Character:** Spectral is a screen-first serif with sharp cuts that holds its personality at 14px, which is what a small window needs; it gives human speech the weight of something written down. Libre Franklin is a Franklin Gothic revival — the sans of American reference publishing — so the chrome reads editorial rather than technical. Spline Sans Mono is narrow, which is an engineering decision as much as an aesthetic one: the minimum window is 460px and prompts need the columns.

### Hierarchy
- **Display** (Spectral 300, 21px, 0.01em): the running head only — the section name at the left of the top bar.
- **Headline** (Spectral 400, 20px, 1.4, balanced): the empty state's lead line.
- **Title** (Spectral 400, 16px, 1.55): the transcript and the extracted objective — the two pieces of human language in a result.
- **Entry** (Spectral 400/italic, 14px): lexicon entries; the italic carries the nonstandard *heard* form, the roman carries the canonical one.
- **Body** (Libre Franklin 400, 13px, 1.5): inputs, field labels, row names.
- **Label** (Libre Franklin 600, 10px, 0.12em, uppercase): every section head, every button, every thumb tab, every usage tag. One label voice across the whole app.
- **Caption** (Libre Franklin 400, 12px, 1.5, max 62ch): explanatory notes under fields.
- **Machine** (Spline Sans Mono 400, 12px, 1.65, tabular): prompts, responses, key legends, counts, percentages, timestamps.

### Named Rules

**The You-Speak-Serif Rule.** Anything a human said or meant sets in Spectral. Anything the machine produced or will paste sets in Spline Sans Mono. Anything the product says about itself sets in Libre Franklin. A transcript in mono or a prompt in serif is a system error, not a style choice.

**The One Label Voice Rule.** There is exactly one label treatment — Libre Franklin 600 / 10px / 0.12em / uppercase — and it does every job: section heads, buttons, tabs, usage tags, state. Introducing a second small-caps style fragments the only device carrying structure.

## Layout

A single fixed-chrome column. A 46px top bar holds the running head at left and the thumb index at right; it is the window drag region. Below it a scrolling page at `calc(100vh - 46px)` with 24px padding, capped to a 620px centred measure.

Window chrome insets are applied **per platform**, not globally: `.head.is-mac` reserves 78px at the left for traffic lights, `.head.is-win` reserves 140px at the right for the Windows caption buttons. Neither platform carries the other's dead space. Platform is detected from the user agent in `App.svelte`.

Spacing is a 4px ramp — 4 / 8 / 12 / 16 / 24 / 32 / 48. Leaves are separated by 32px, fields by 24px, label-to-content by 12px. Sections get more space above than below.

Two breakpoints, both driven by the app's real window sizes (540×660 default, 460×560 minimum): at ≤540px padding drops to 16px and paired fields stack; at ≤470px the running head is dropped and the thumb index spreads to full width, and the empty state's guide collapses to a single column.

**The Ruled Head Rule.** Every block in the app — a result section, a settings group, a model group, the lexicon — opens with the same object: a 10px uppercase label, a 1px hairline that flexes to fill the remaining width, and optional right-aligned mono metadata. This one component replaced every card, group, and row container in the previous system. It costs no vertical space and it is what makes a settings screen scannable.

## Elevation & Depth

The main window has **no shadows at all**. Depth is carried entirely by three ink tones, two rule weights, and type. No card sheen, no gradient, no glass, no glow. This is the deliberate inverse of the previous system, whose ambient shadows communicated nothing and whose card sheen simulated a light source the interface does not have.

The floating overlay is the single exception, and for a functional reason: it sits over arbitrary application content and needs real separation from an unknown background. It carries an offset-and-blur shadow plus a low-alpha bone rule, verified legible over both a dark editor and a light document.

### Shadow Vocabulary
- **Slip** (`box-shadow: 0 10px 30px rgba(0,0,0,0.55), 0 1px 3px rgba(0,0,0,0.6)`): the floating overlay only. Never used in the main window.

### Named Rules

**The Flat Page Rule.** No element in the main window casts a shadow. If something needs to separate from its surroundings, it gets a rule or a tonal step, not a shadow.

## Shapes

Radius is `0`. Every container, button, input, key legend, checkbox, list row, and progress bar is a true rectangle.

There is exactly one curve in the application: the 60px record circle, kept round because a round record button is a physical affordance people rely on across every recording tool. Its inner mark is the second exception — an 18px dot that scales to 84% and squares off while recording, which is the universal dot-to-square record language.

Borders are always 1px, with two deliberate exceptions: the active thumb tab and the error rule are 2px, because both are marks.

The dominant silhouette is a hairline running the width of the measure with a label sitting at its left end.

### Named Rules

**The One Round Thing Rule.** The record button is the only curve in the system. Anything else that arrives with a radius has imported another world's habits.

**The Ruled-Line Field Rule.** An editable field is a line you write on, never a box you type into: transparent background, a single bottom rule, no border on the other three sides. Focus turns the rule proof-red and thickens it with an inset shadow rather than a border change, so nothing shifts by a pixel when tabbing through a form.

## Components

### Buttons
- **Shape:** rectangle, 0 radius, 10px uppercase label at 0.1em, 36px minimum height.
- **Primary:** bone fill, ink text. Hover goes to pure white; active steps to bone-2. The affirmative key is the brightest thing on the screen — that is its job, and it is why red does not need to be.
- **Ghost:** transparent, bone-2 text, rule-strong border. Hover brightens the text and border; active takes the ink-pressed fill.
- **Danger:** transparent, proof text, proof-deep border. Never filled.
- **Disabled:** no fill, bone-4 text, rule border.
- **Small:** 30px height, 9px label. **Icon:** 30px square.

### Text Buttons
Borderless 10px uppercase in bone-3 with a transparent bottom border that becomes proof-red on hover. Used for in-row operations: Copy, Paste, Delete, Cancel, Reset to default.

**The Quiet Destructive Rule.** A destructive text action looks exactly like its neighbours at rest and only takes the mark on hover. A column of red Deletes down a history list makes the most dangerous action the most salient thing on the page.

### Fields
Transparent, 1px bottom rule in rule-strong, 38px minimum height, 13px Libre Franklin. Hover lifts the rule to bone-4; focus turns it proof-red plus an inset 1px proof shadow. Selects strip the native appearance and supply an inline SVG chevron in bone-2 at the right edge; the option list is left to the OS with an ink-well background hint. Labels sit above at 12px/500; notes sit below at 12px in bone-3, capped at 62ch.

### Checkbox
A 15px square with a rule-strong border containing a 9px proof-red square that scales from 0 to 1 when checked. Declared as `label.check-row` specifically to outrank the `.field > label { display: block }` rule, which would otherwise stack the box above its own caption.

### Thumb Index (navigation)
Five uppercase 10px tabs at the right of the top bar. Default bone-3; hover and active bone; the active tab carries a 2px proof-red bottom border. `aria-current="page"` marks the active section.

### Options (segmented choice)
The same interaction language as the thumb index, reused rather than reinvented: a row of 11px capitalised text buttons, the active one carrying a proof-red 2px underline and 600 weight. There is no track, no pill, no fill. Used for optimization mode and output mode.

### The Leaf (signature)
The system's only container. A `.leaf-head` — label, flexing hairline, optional mono metadata — over its content, with 32px between leaves. Every result section, settings group, model group, and lexicon block is one of these. There are no cards anywhere in the application.

### The Entry (signature)
A result, set as a dictionary entry: **Heard** (transcript in Spectral, then the correction marks, then Re-correct with AI), **Understood** (the objective in Spectral, then a usage line of middot-separated uppercase tags), **Optimized prompt** (mode and token estimate right-aligned in the head; the prompt in mono behind a 1px left rule with a 16px indent; Send to LLM and Copy), and **Response** when present. Machine blocks are indented under a rule rather than boxed, the way a quoted passage is set, and are selectable text.

### The Mark (signature)
The correction, and the most important object in the product. The *heard* form sets in italic Spectral in bone-3 — dictionary convention for a nonstandard variant — followed by a drawn proof-red arrow and the canonical form in proof-red at 500. LLM-tier fixes carry an inline 9px `Save` action that writes the pair into the user's lexicon. The whole mark strikes in over 180ms with a left-to-right `clip-path` reveal, like a pen stroke.

### The Lexicon List
A four-column grid — `minmax(0,1fr) auto minmax(0,1fr) auto` — so every arrow lands on the same vertical axis down the page. That column alignment is what makes it read as a dictionary rather than a list of pairs.

### Key Legend
`kbd` as a printed legend: 24px tall, minimum 24px wide, ink-well fill, rule-strong border, 0 radius, 11px mono. No gradient and no faked keycap edge.

### Record Control
A 60px circle with a 1px rule-strong border and no fill, containing an 18px proof-red dot. Hover lifts the border to bone-2. Recording turns the border proof-red and scales the dot to 84% with the radius going to 0. Transcribing greys the mark and disables the control. Beside it: the state in 10px uppercase, the live rule, and the hotkey legend.

### The Live Rule (signature)
A 2px track carrying a proof-red fill whose `scaleX` follows real capture level from `pie://level`, transitioned at 90ms. Without a level feed it falls back to a 1.6s sweep so the surface never looks frozen; under reduced motion it becomes a static full-width rule. Transcribing replaces it with a hatched repeating gradient.

### The Slip (the overlay)
The surface users actually see. A flat ink card at 97% opacity, 1px bone rule at 22% alpha, 0 radius, the slip shadow, containing the same mark vocabulary as the record button, the state in 10px uppercase, and the live rule. It imports `ui/src/tokens.css`, so it and the main window are the same product by construction rather than by discipline.

### Error
A 2px proof-red rule above a row of: the word ERROR in proof, the message in bone at 12px, and a drawn close icon in bone-3. No tinted panel, no boxed alert.

### Icons
Drawn in `ui/src/lib/Icon.svelte` on a 16px grid at 1.4px stroke with **square caps and mitre joins**, because the interface has no rounded corners and its icons should not either. No icon library, no unicode glyphs standing in for icons.

## Do's and Don'ts

### Do:
- **Do** keep every token in `ui/src/tokens.css` and import it into both surfaces. The overlay sharing the main window's tokens is a structural fix, not a convenience.
- **Do** open every block with a `.leaf-head`. It is the system's only container and its only structural device.
- **Do** set human speech in Spectral and machine output in Spline Sans Mono, always.
- **Do** keep proof-red to marks and rules, under 2% of any screen.
- **Do** signal state with shape and a word as well as colour.
- **Do** keep the global `:focus-visible` (2px proof outline, 2px offset) and the `prefers-reduced-motion` neutraliser — and keep the recording rule visible under reduced motion, since it is a state signal rather than decoration.
- **Do** apply window-chrome insets per platform (`.is-mac`, `.is-win`), never as a global constant.
- **Do** animate with `transform` and `opacity` only; progress and the level meter fill by `scaleX` via a `--fill` custom property.

### Don't:
- **Don't** add a card, a panel, a tinted callout, or any container with a background and a border. The system has one container and it has neither.
- **Don't** add a radius. The record circle is the only curve, and it is spoken for.
- **Don't** add a shadow to anything in the main window. The overlay's is functional and singular.
- **Don't** fill anything with proof-red — not a button, not a badge, not a selected row. A full-width red rule under a selected list row was tried and removed for exactly this reason.
- **Don't** colour destructive text actions red at rest.
- **Don't** set body copy in `--bone-4`; it is below the 4.5:1 floor.
- **Don't** introduce a second label style, a gradient, a glass or blur effect, or a unicode glyph in place of a drawn icon.
- **Don't** reintroduce the previous world's indigo (`#6366f1`), 12px radii, gradient buttons, or card sheen. They were removed deliberately.
