# kno

`kno` is a cli for organizing your notes in plaintext, inspired by `pass` the unix password manager.

Your notes stay in plain text (markdown) files.
`kno` keeps everything organized and in a folder structure 
and handles version control with git for you.

By keeping all the notes in a single directory as plain text,
it's super easy to use your favorite text editor and unix commands
to manage searching, syncing, encryption.

I have tried using vimwiki, joplin, and just plain text files in working folders or centralized folders.
Vimwiki tried to be too much, and having a custom markdown format and mysterious linking behaviour annoyed me.
Joplin is simple enough but it's not straight forward to version control and i don't get the full power of vim.
Plaintext files are fine, but _where are they_? Did i put them in a `/notes` directory in `/~`? Are they scattered throughout all of my project folders? How do I search them?

By using `kno` I centralize all my notes but also make them accessible from any place. Its easy to search the plaintext either in vim with `telescope`, in the shell with `grep`, or tell an llm to look for them.

## Usage

```bash
# first-time setup: creates ~/.kno, initializes git, sets up shell completions
kno init

# open today's daily note in your $EDITOR
kno

# open (or create) a named note
kno sql/joins

# trailing slash = directory with a daily-dated file inside
kno work/standup/

# append a line without opening the editor
kno -a "remember to fix the auth bug"
kno sql/joins -a "- LEFT JOIN keeps all rows from the left table"

# print the resolved file path instead of opening the editor
kno -p                # prints e.g. /home/you/.kno/daily/2026/2026-02-15.md
kno sql/joins -p      # prints e.g. /home/you/.kno/sql/joins.md

# list notes (tree view, depth 1 by default)
kno list
kno list sql
kno list -L 0         # unlimited depth
kno list -L 2         # two levels deep

# git passthrough — any git command, run against ~/.kno
kno git status
kno git add -A && kno git commit -m "save notes"
kno git log --oneline
```

### Vim integration

Add to your vimrc to open today's note with `<leader>kn`:

```vim
nnoremap <leader>kn :execute 'e' trim(system('kno -p'))<CR>
```
