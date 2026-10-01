---
name: wcag-accessibility-checks
description: Use when you plan or check an interface change for WCAG 2.2 AA accessibility, including reading an automated accessibility check's results.
---

# Checking WCAG 2.2 AA

Every screen you change meets WCAG 2.2 level AA. An automated check finds some failures; the rest
you check by reading the code and the screen.

## 1. Read the automated results

When your session offers `farik_check_page`, run it for each screen at phone and desktop width, in
the light and dark themes. Each violation names its rule, its impact, the element and a help text.
Fix every `critical` and `serious` one, and name any other in the completion note. When your session
has no such tool, check the rules below from the code.

## 2. What to check

- **Contrast**: text 4.5:1 against its background, large text and controls' borders and focus
  rings 3:1. Use the project's token pairs, which are already measured; a new pair needs its ratio.
- **Focus**: every control is reachable by keyboard, in a sensible order, with a visible focus ring
  that is never removed.
- **Target size** (2.5.8): every pointer target is at least 24 by 24 CSS pixels, or has that much
  space around it.
- **Labels and names**: every control has a visible label and an accessible name that contains it;
  an icon-only button has a name; images carry alternative text or are marked decorative.
- **Reflow**: at 320 CSS pixels wide the content fits with no sideways scroll, and nothing is lost.
- **Motion**: nothing moves on its own for more than five seconds without a way to stop it, and
  animation respects `prefers-reduced-motion`.

## 3. What no tool sees

An automated check cannot tell whether the order of reading makes sense, whether an error message
helps, whether a label says what the control does, or whether colour alone carries a meaning.
Check those by eye on every screen, and say in the completion note that you did.
