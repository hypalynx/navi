# Navi

_A shell-like, scriptable interface for working with LLMs._

Navi is a program you can use to interact with LLMs on the
command line in the same way you might interact with Ruby (via
`irb`) or Node (via.. `node`).

I created this program to support a workflow where programming is
done mainly by the human programmer, it's intended to work with
small, local models such as `Qwen 3.5 9b`. Something of this size
is really useful (see use cases below) and capable of giving fast
feedback.

## Getting Started

Download the project and `make install`, later I will be
providing releases with pre-compiled binaries.

### Configuration

Before you get started, you'll need to configure Navi to connect
to an LLM. I'd recommend downloading a copy of Qwen 3.5 9B from
Huggingface and running it with llama.cpp.

Once you have a local LLM running, connect to it by setting:

`server: 127.0.0.1:7777`

### Using `navi`

- `navi` by itself will launch a prompt for you to talk to an LLM
  (configured in your ~/.config/navi/navi.yaml)
- `navi -e "How do I relax?"` to send text to the LLM server.
- `cat stacktrace.log | navi -e "help me find out where this
  error occurred"`

## Use Cases

1. Review of code and/or text content.
2. Searching for information (either in model or by searching the
   internet) and collating the results.
3. Discussing code patterns and providing snippets (i.e
   interactive StackOverflow).
4. Writing small scripts

Bigger models can drive entire PRs from single prompts
(one-shots) which is very powerful, but does not help maintain
programs as humans are left out of the loop. We're all still
learning how to take advantage of LLMs but I'm experimenting with
a much 


## Tasks / Features to implement

- `$` execute your own shell command like `@`
- `@` address a file

## Technical Tasks
- `needs_spacing` & `had_thinking` probably don't need to exist
  in Renderer.
- split render into markdown/layout/syntax submods
- use in neovim? i.g visual highlight and then prompt for extra
  context? mini/shadow term like conjure?
- line by line code highlight? or streamed like spartan neovim?
  i.e just highlight strings etc.
- history broken?
- don't put . full stop etc on newlines when wrapping.. more
  intelligent wrapping in general
