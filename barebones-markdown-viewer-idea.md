# Marky

This is an idea file. Its job is to communicate the idea clearly and keep scope tight.

## The core idea

Build the most basic, barebones, lightweight, and performant desktop markdown viewer possible for Windows.

This is not a notes app.  
This is not an editor-first app.  
This is not Obsidian-lite.  
This is not Electron bloat.

The goal is simple: I want to open `.md` files in peace.

It should boot fast, feel tiny, render markdown cleanly, and let me toggle between rendered view and raw source. It should be good enough that I can set it as my default app for `.md` files on Windows and just use it every day.

## What it should do

- Open local markdown files fast
- Render markdown with clean, readable formatting
- Toggle between:
  - rendered markdown view
  - raw source view
- Support basic markdown features well:
  - headings
  - paragraphs
  - lists
  - code blocks
  - inline code
  - links
  - blockquotes
  - tables if reasonable
- Let me open files from:
  - double-clicking a `.md` file
  - drag and drop
  - file picker
- Remember very small quality-of-life things only if they are basically free:
  - window size
  - last view mode
  - maybe theme preference

## Constraints

Optimize for:
1. fast startup
2. low memory usage
3. dead simple code
4. minimal dependencies
5. a clean desktop feel on Windows

Avoid:
- Electron
- heavy web stacks unless there is a very strong reason
- plugin systems
- workspace features
- tabs unless they are basically free
- sync
- database
- note graph
- AI features
- file indexing
- anything “productized” beyond the tiny core app

This should feel closer to a tiny utility than a platform.

## Challenge framing

Treat this like a performance and simplicity challenge.

Try to make the app:
- extremely small in scope
- easy to build
- easy to package
- easy to set as the default `.md` opener on Windows
- pleasant to read in

Choose the implementation approach that best matches that goal. Favor the simplest stack that gives a native-feeling result and fast boot time.

## Codex instructions

Build this directly. Do not spend time in plan mode unless you hit a real blocker.

Before writing code:
- inspect the repo quickly
- see whether there is already a preferred stack or existing desktop app scaffold
- if the repo is empty or flexible, choose the lightest reasonable Windows-first stack

Prefer a small native or near-native approach. If you choose something heavier, justify it briefly in the README.

Ask me a question only if one missing decision blocks implementation. Otherwise, proceed.

## Deliverables

Produce:
1. the app
2. a short README with:
   - how to run it
   - how to build it
   - how to set it as the default app for `.md` files on Windows
3. a short note explaining the stack choice in terms of speed, simplicity, and footprint

## UX bar

The app should feel calm and clean.

Good typography matters.  
Spacing matters.  
Rendered markdown should look nice by default without turning this into a design project.

Keep the UI minimal:
- open
- view toggle
- maybe reload
- maybe theme toggle

That’s it.

## Out of scope

Do not turn this into:
- a full markdown editor
- a docs site generator
- a knowledge base app
- a developer IDE
- a browser shell for a web app
- a feature showcase

Stay disciplined.

## Final instruction to Codex

Build the smallest good version of this.

Bias toward shipping a tiny, solid markdown viewer over building a flexible architecture.
