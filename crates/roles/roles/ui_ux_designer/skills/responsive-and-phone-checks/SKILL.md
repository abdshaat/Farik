---
name: responsive-and-phone-checks
description: Use when you plan or check a screen at phone and desktop widths, for touch targets, sideways scroll and the phone's bottom bar.
---

# Checking phone and desktop widths

Every screen works at every width a person uses. Check the ones below for each screen you change.

## The widths

- **360 px**: the narrowest phone you design for. Nothing scrolls sideways, and nothing is cut off.
- **390 px**: a common phone. The layout is the phone layout, with room to spare.
- **1280 px**: the desktop. The content does not stretch into lines too long to read.

## At each width

- **No sideways scroll.** Long words, tables, code and images wrap, shrink or scroll inside their
  own box, never the page.
- **Touch targets** are at least 44 by 44 CSS pixels on a phone, with space between neighbours, and
  never less than 24 by 24 (WCAG 2.5.8).
- **The phone's bottom bar**: if the app has one, nothing important sits under it, and the last item
  on a page can be scrolled clear of it.
- **Order**: what stacks on a phone stacks in reading order, with the main action easy to reach.
- **Both themes** at each width, since a colour that works in one can fail in the other.

## Writing it down

In the plan, say which widths you checked and what failed at each. In the completion note, say
which widths you checked after the change and what you saw.
