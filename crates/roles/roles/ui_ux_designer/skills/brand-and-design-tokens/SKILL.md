---
name: brand-and-design-tokens
description: Use when you plan or make a visual change and need a colour, a size, a font or a spacing, so that it comes from the project's brand and design tokens.
---

# Using the brand and the design tokens

The project has already decided how it looks. Your job is to use those decisions, not to make new
ones in the code.

## 1. Find the tokens

Before you plan, find where the project keeps its brand and its design tokens: a tokens file, a
theme, CSS custom properties, a component library. Read the brand's written rules if it has them.
For Farik itself they are the `brand` package of the `farik` scope (`packages/brand/tokens/tokens.json`) and
`docs/brand/brand.md`, and the components in its `ui` package.

## 2. Never invent a value

- A colour is a token, never a literal. A size, a spacing, a radius and a font come from the scale.
- A component the project already has is used before a new one is made.
- Each colour has one job. Do not borrow a colour for a job it does not have because it looks
  right; a status colour on a button tells the user something false.
- Both themes: a change that works in the light theme is checked in the dark one.

## 3. When nothing fits

If the design needs a value the tokens do not have, do not add one quietly. Say so in the plan:
what is missing, why none of the existing tokens fits, and the value you propose. The Product
Manager decides, and a new token is its own change.
