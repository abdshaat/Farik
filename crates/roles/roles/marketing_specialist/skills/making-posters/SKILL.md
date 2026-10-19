---
name: making-posters
description: Use when the task asks for a poster, a social card or a video storyboard. A poster is an SVG under docs/catervas/marketing/assets/posters/, made from the brand kit.
---

# Making posters

A poster is a file you can open and print or post: an SVG saved under
`docs/catervas/marketing/assets/posters/`. You write no application code; an SVG is a drawing.
`making-images-and-video` makes pictures and clips with Higgsfield and Recraft; this skill lays
words and shapes out.

## 1. Start from the brand kit

Read `docs/catervas/marketing/brand/brand-kit` first: its colours, its type, its logo (the
owner's own file) and the picture style. If it does not exist, make the brand kit first
(`keeping-the-brand-kit`) or ask the owner. Use only those colours and fonts, and the logo as it
is, never redrawn.

## 2. One idea, before the layout

Write, in three or four lines at the top of your note: what the poster is for, who sees it
where, the one thing they should take away, and the feeling (calm, bold, playful). A poster says
one thing. If you need two, make two.

## 3. The words are exact

Write the words first and have them right: spelling, prices, dates, the address, names. Keep them
few: a headline, one line of support, one call to act, the logo. Say where each line is, and keep
the type large enough to read at the size it is seen. Never put a claim on a poster that the
business cannot show, and no false deadline or "only 3 left".

## 4. Sizes

Pick the size for where it goes: a square for a feed post, tall for stories and shorts, wide for
a link card (about 1200 by 630) or a header, and the printed paper size for a print. Leave a
margin. State the size in the file's `viewBox`.

## 5. Draw it

Build the SVG from rectangles, circles, paths and text, with a clear order from top to bottom of
what the eye reads: the headline, the picture or shape, the support, the logo. Put the words as
text elements, with the brand's font names and a plain fallback. A picture made with Higgsfield or
Recraft may sit inside as an image at a public address or an embedded file; say what made it.
Check the result opens, the text does not overflow, and the colours read against each other.

## 6. A video starts as a storyboard

Write a storyboard document first: one row for each scene, with the words on screen, the voice-over,
what is shown, and how long. The first three seconds must carry the hook, since many viewers
leave then. Give a length for each network. Make the clip with Higgsfield only once the storyboard
is fixed, and say what it will use of the allowance first.

## 7. Where it goes

A poster for a post is attached to a post in the marketing plan and goes out through
`catervas_schedule_post`; a poster for print is for the owner to print. List what you made, where,
and what it is for, in your completion note.

## Drawn from

Anthropic, skills, `canvas-design` (Apache-2.0: a short idea written before the layout); Corey
Haines, marketingskills (MIT): `image` (sizes and the choice between drawn, made and screenshot
pictures) and `video` (a storyboard first, the first three seconds). Rewritten for Catervas, with
the brand kit's colours and type in place of a free choice.
