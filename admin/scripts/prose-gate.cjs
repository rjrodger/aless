
'use strict'

const Fs = require('node:fs')
const Path = require('node:path')

const REPO = Path.join(__dirname, '..', '..')

const CONFIG = {
  pages: [],
  tutorials: [],
  contributor: [],
  links: ['**/*.md'],
  skip: ['node_modules', '.git', 'sketch'],
  names: [],
  internal: [],
  guide: 'STYLE-GUIDE.md',
  vocabulary: '.vale/styles/config/vocabularies/House',
  ...JSON.parse(Fs.readFileSync(Path.join(__dirname, 'prose-gate.json'), 'utf8')),
}

const SKIP_DIRS = new Set(CONFIG.skip)
const REJECT = `${CONFIG.vocabulary}/reject.txt`


function globRe(glob) {
  let re = ''
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i]
    if ('*' === c && '*' === glob[i + 1]) {
      i++
      if ('/' === glob[i + 1]) {
        i++
        re += '(?:.*/)?'
      }
      else {
        re += '.*'
      }
    }
    else if ('*' === c) re += '[^/]*'
    else if ('?' === c) re += '[^/]'
    else re += c.replace(/[.+^${}()|[\]\\]/g, '\\$&')
  }
  return new RegExp(`^${re}$`)
}


function matcher(globs) {
  const res = globs.map(globRe)
  return (file) => res.some((re) => re.test(file))
}

const isTutorial = matcher(CONFIG.tutorials)
const isContributor = matcher(CONFIG.contributor)
const isLinked = matcher(CONFIG.links)


// A symbolic link is skipped: `CLAUDE.md` names `AGENTS.md`, which is
// read as itself.
function walk(dir, out) {
  const abs = '' === dir ? REPO : Path.join(REPO, dir)
  for (const entry of Fs.readdirSync(abs, { withFileTypes: true })) {
    const rel = '' === dir ? entry.name : `${dir}/${entry.name}`
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) walk(rel, out)
    }
    else if (entry.isFile() && entry.name.endsWith('.md')) {
      out.push(rel)
    }
  }
  return out
}


function markdownFiles() {
  return walk('', []).sort()
}


// The style guide quotes every banned phrase in order to ban it.
function pages() {
  const all = markdownFiles()
  const out = []
  for (const glob of CONFIG.pages) {
    const re = globRe(glob)
    for (const file of all) {
      if (re.test(file) && file !== CONFIG.guide && !out.includes(file)) out.push(file)
    }
  }
  return out
}


function lf(text) {
  return text.replaceAll('\r\n', '\n').replaceAll('\r', '\n')
}


// A fence inside a list item is indented as deep as the item's text,
// so the indent is not bounded.
const FENCE_OPEN = /^(\s*)(`{3,}|~{3,})[ \t]*([^`\s]*)[^`]*$/

function fenceCloser(fence) {
  return new RegExp(`^\\s*${fence[0]}{${fence.length},}\\s*$`)
}


// Every fenced block is blanked, keeping the line count, so a reported
// line still opens on the offending line.
function fenceless(md) {
  const lines = lf(md).split('\n')
  const out = [...lines]
  for (let i = 0; i < lines.length; i++) {
    const fm = lines[i].match(FENCE_OPEN)
    if (!fm) continue
    const closer = fenceCloser(fm[2])
    out[i] = ''
    let j = i + 1
    for (; j < lines.length && !closer.test(lines[j]); j++) out[j] = ''
    if (j < lines.length) out[j] = ''
    i = j
  }
  return out.join('\n')
}


// The delimiter is a RUN of backticks, maximal at both ends, and stops
// at a newline. Without either guard the match shrinks or runs on, and
// the prose it removes was never a code span.
const CODE_SPAN = /(?<!`)(`+)(?!`)(?:[^`\n]|(?!\1)`)*(?<!`)\1(?!`)/g

// A span that wraps once, which the newline bound above leaves whole.
const CODE_WRAP =
  /(?<!`)(`+)(?!`)(?:[^`\n]|(?!\1)`)*\n(?:[^`\n]|(?!\1)`)*(?<!`)\1(?!`)/g

// A warning sign or an arrow is text presentation; a keycap or a flag
// sits in no symbol block.
const EMOJI = /\p{Emoji_Presentation}|️|⃣|[\u{1F1E6}-\u{1F1FF}]/u

// `I` is a pronoun only capitalised, since a lone `i` is the one in
// `i.e.`; `my` is one however it falls. `I/O` is neither.
const FIRST_I = /\bI(?!\/O)\b|\bI'(?:m|ve|ll|d)\b/
const FIRST_MY = /\b(?:my|mine|myself)\b/i

// Case-sensitive, so that an initialism spelled in capitals is not
// read as a pronoun.
const FIRST_WE = /\b(?:[Ww]e|[Uu]s|[Oo]urs?|[Ll]et['’]s)\b/

// Every mark except `!=` and the `!` opening an image.
const EXCLAMATION = /!(?![=[])/g

// A destination is not prose: the `!` in `](/a!b)` would spend a page's
// exclamation ration on a URL character.
const DESTINATION = /(\]\()[^)\n]*\)/g
const AUTOLINK = /<https?:\/\/[^\s<>]*>/g
const URL = /\bhttps?:\/\/[^\s<>)\]]*[^\s<>)\]!.,;:?*_"']/g

const blank = (text) => text.replace(/[^\n]/g, '')


function firstSingular(line) {
  const found = line.match(FIRST_I) || line.match(FIRST_MY)
  return found ? found[0] : null
}


function prose(md) {
  return fenceless(md)
    .replace(/^---\n[\s\S]*?\n---\n/, blank)
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(CODE_SPAN, '')
    .replace(CODE_WRAP, blank)
    .replace(DESTINATION, '$1)')
    .replace(AUTOLINK, '')
    .replace(URL, '')
}


// Markdown needs no blank line before a block, so `## Something worth`
// above `noting this` would join and report `worth noting`. A heading,
// table row and rule close a paragraph as well as open one.
const OPENS = /^\s*(?:[-*+] |\d+[.)] |#{1,6} |>|`{3,}|~{3,})/
const CLOSES = /^\s*(?:#{1,6} |(?:[-*_] *){3,}$)/

// A single-column table writes `| cell`, one pipe, the shape of a
// sentence opening with one; the delimiter row tells them apart.
const PIPED = /^\s*\|/
const DELIMITER = /^\s*\|[-:| ]*-[-:| ]*$/


function tableRows(lines) {
  const rows = new Set()
  for (let i = 0; i < lines.length; i++) {
    if (!PIPED.test(lines[i])) continue
    let end = i
    while (end < lines.length && PIPED.test(lines[end])) end++
    const table = lines.slice(i, end).some((l) => DELIMITER.test(l))
    for (let n = i; n < end; n++) {
      if (table || 1 < (lines[n].match(/\|/g) || []).length) rows.add(n)
    }
    i = end - 1
  }
  return rows
}


// A paragraph joined for matching, so a phrase split by a line wrap is
// still found, with each piece's physical line kept for the report.
function logical(text) {
  const out = []
  let pieces = []
  let starts = []
  let lines = []
  let at = 0

  const flush = () => {
    if (0 < pieces.length) {
      out.push({ text: pieces.join(' '), starts, lines, pieces })
      pieces = []
      starts = []
      lines = []
      at = 0
    }
  }

  const split = lf(text).split('\n')
  const rows = tableRows(split)

  split.forEach((line, i) => {
    if ('' === line.trim()) {
      flush()
      return
    }
    if (OPENS.test(line) || rows.has(i)) flush()
    const piece = line.trim().replace(/\s+/g, ' ')
    starts.push(at)
    lines.push(i + 1)
    pieces.push(piece)
    at += piece.length + 1
    if (CLOSES.test(line) || rows.has(i)) flush()
  })
  flush()

  return out
}


function lineAt(para, index) {
  let k = 0
  for (let n = 0; n < para.starts.length; n++) {
    if (para.starts[n] <= index) k = n
  }
  return { line: para.lines[k], text: para.pieces[k] }
}


let banned = null

// Vale reads every non-blank line as a pattern, since a vocabulary file
// has no comment syntax. A heading is inert; a line that is only `#`
// bans the character, and is refused.
function bannedList() {
  if (banned) return banned
  const lines = Fs.readFileSync(Path.join(REPO, REJECT), 'utf8')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => '' !== line)
  if (lines.some((line) => '#' === line)) {
    throw new Error(`${REJECT}: a line that is only "#" bans the character`)
  }
  if (0 === lines.length) {
    throw new Error(`${REJECT}: no patterns, so the phrase gate is off`)
  }
  banned = lines.map((pat) => [new RegExp(`\\b(?:${pat})\\b`, 'gi'), pat])
  return banned
}


const INTERNAL = [
  [/\bADR-\d+\b/g, 'a decision record'],
  [/\b(?:the|an|this|that) ADR\b/gi, 'a decision record'],
  [/\bADR\.md\b/g, 'the decision record'],
  [/\bsketch\//g, 'a working note'],
  [/\bdesign note\b/gi, 'a design note'],
  [/\b[Dd]ecisions? \d+\b/g, 'a numbered decision'],
  [/(?<![\w./-])(?:\.\.\/)*admin\//g, 'an admin note'],
  [/\]\([^)]*\b(?:AGENTS|CLAUDE|DEV-PROCESS|STYLE-GUIDE)\.md[^)]*\)/g,
    'a link to a contributor guide'],
  ...CONFIG.internal.map((entry) => [
    new RegExp(entry.pattern, [...new Set(`g${entry.flags || ''}`)].join('')),
    entry.what]),
]

const NAMES = CONFIG.names.map((entry) => [new RegExp(entry.pattern), entry.spelled])


function checkPage(file, text) {
  const findings = []
  const add = (line, rule, detail, excerpt) => {
    const finding = { file, line, rule, detail, excerpt: excerpt.trim().slice(0, 72) }
    if (!findings.some((f) => f.line === line && f.rule === rule && f.detail === detail)) {
      findings.push(finding)
    }
  }

  const md = lf(text)
  const body = prose(md)
  const tutorial = isTutorial(file)

  for (const para of logical(body)) {
    for (const [re, name] of bannedList()) {
      for (const m of para.text.matchAll(re)) {
        const { line, text: piece } = lineAt(para, m.index)
        add(line, 'banned-phrase', `"${name}"`, piece)
      }
    }
  }

  let marks = 0
  body.split('\n').forEach((line, i) => {
    if (line.includes('—')) add(i + 1, 'em-dash', 'an em dash in prose', line)

    const we = tutorial ? null : line.match(FIRST_WE)
    if (we) add(i + 1, 'first-person-plural', `"${we[0]}" outside a tutorial`, line)

    const me = firstSingular(line)
    if (me) add(i + 1, 'first-person-singular', `"${me}"`, line)

    for (const [re, spelled] of NAMES) {
      const m = line.match(re)
      if (m) add(i + 1, 'name-spelling', `"${m[0]}" is spelled ${spelled}`, line)
    }

    for (const m of line.matchAll(EXCLAMATION)) {
      marks++
      if (!tutorial) add(i + 1, 'exclamation', `a mark outside a tutorial, at ${m.index}`, line)
      else if (1 < marks) add(i + 1, 'exclamation', `mark ${marks}, and a tutorial gets one`, line)
    }
  })

  md.split('\n').forEach((line, i) => {
    if (EMOJI.test(line)) add(i + 1, 'emoji', 'an emoji', line)
  })

  if (!isContributor(file)) {
    for (const para of logical(md)) {
      for (const [re, what] of INTERNAL) {
        for (const m of para.text.matchAll(re)) {
          const { line, text: piece } = lineAt(para, m.index)
          add(line, 'internal-citation', `cites ${what}`, piece)
        }
      }
    }
  }

  return findings
}


// GitHub's heading anchor: lower-cased, punctuation dropped except `-`
// and `_`, each space a hyphen, a repeat suffixed by its occurrence
// count.
function slug(heading) {
  return heading.toLowerCase()
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[^\p{L}\p{M}\p{N} _-]/gu, '')
    .replace(/ /g, '-')
}


const SETEXT = /^ {0,3}(?:=+|-+)[ \t]*$/

const inParagraph = (line) => '' !== line.trim()
  && !OPENS.test(line) && !PIPED.test(line) && !SETEXT.test(line)


// A Setext heading is the paragraph above its underline, however many
// lines that paragraph wraps to.
function anchors(md) {
  const seen = new Map()
  const out = new Set()
  const add = (heading) => {
    const base = slug(heading)
    const n = seen.get(base) || 0
    seen.set(base, n + 1)
    out.add(0 === n ? base : `${base}-${n}`)
  }
  const lines = fenceless(md).replace(/^---\n[\s\S]*?\n---\n/, blank).split('\n')
  lines.forEach((line, i) => {
    const atx = line.match(/^#{1,6}\s+(.*?)(?:\s+#+)?\s*$/)
    if (atx) {
      add(atx[1])
      return
    }
    if (!SETEXT.test(line)) return
    let start = i
    while (0 < start && inParagraph(lines[start - 1])) start--
    if (start < i) add(lines.slice(start, i).map((l) => l.trim()).join(' '))
  })
  for (const m of md.matchAll(/<a\s[^>]*\b(?:id|name)="([^"]+)"/g)) out.add(m[1])
  return out
}


const LINK = /\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g
const SCHEME = /^[a-z][a-z0-9+.-]*:/i

// A reference definition, `[name]: destination`. A footnote, `[^1]:`,
// is text, not a destination.
const DEFINITION = /^ {0,3}\[(?!\^)[^\]]+\]:\s*<?([^\s>]+)>?/

function decode(text) {
  try {
    return decodeURIComponent(text)
  }
  catch {
    return text
  }
}


const anchorCache = new Map()

function anchorsOf(abs) {
  if (!anchorCache.has(abs)) anchorCache.set(abs, anchors(Fs.readFileSync(abs, 'utf8')))
  return anchorCache.get(abs)
}


// A link shown in a code span or a fence is an example, not a link. A
// path from `/` is read from the repository root, as GitHub reads it.
function checkLinks(file, text) {
  const findings = []
  const abs = Path.join(REPO, file)
  const md = fenceless(text).replace(CODE_SPAN, '')
  md.split('\n').forEach((line, i) => {
    const definition = line.match(DEFINITION)
    const hrefs = [...line.matchAll(LINK)].map((m) => m[1])
    if (definition) hrefs.push(definition[1])
    for (const href of hrefs) {
      if (SCHEME.test(href) || href.startsWith('//')) continue
      const hash = href.indexOf('#')
      const rel = decode(-1 === hash ? href : href.slice(0, hash))
      const frag = -1 === hash ? '' : decode(href.slice(hash + 1))
      const target = '' === rel ? abs
        : rel.startsWith('/') ? Path.join(REPO, rel)
          : Path.resolve(Path.dirname(abs), rel)
      const add = (detail) => findings.push(
        { file, line: i + 1, rule: 'dead-link', detail: `${href}: ${detail}`, excerpt: line.trim().slice(0, 72) })

      if (target !== REPO && !target.startsWith(REPO + Path.sep)) add('outside the repository')
      else if (!Fs.existsSync(target)) add('no such file')
      else if ('' !== frag && target.endsWith('.md') && !anchorsOf(target).has(frag)) add('no such heading')
    }
  })
  return findings
}


// Every category of the banned list has its summary in the guide, so a
// whole category cannot be added to one and missed by the other.
function checkGuide() {
  const guide = Path.join(REPO, CONFIG.guide)
  if (!Fs.existsSync(guide)) return []
  const summary = Fs.readFileSync(guide, 'utf8').toLowerCase()
  const findings = []
  Fs.readFileSync(Path.join(REPO, REJECT), 'utf8').split('\n').forEach((line, i) => {
    const m = line.match(/^# --- (.+?) -+$/)
    if (m && !summary.includes(m[1].trim().toLowerCase())) {
      findings.push({ file: REJECT, line: i + 1, rule: 'guide-category',
        detail: `"${m[1].trim()}" has no summary in ${CONFIG.guide}`, excerpt: '' })
    }
  })
  return findings
}


function read(file) {
  return Fs.readFileSync(Path.join(REPO, file), 'utf8')
}


function checkAll(only) {
  const gated = only || pages()
  const linked = only || markdownFiles().filter(isLinked)
  return [
    ...gated.flatMap((file) => checkPage(file, read(file))),
    ...linked.flatMap((file) => checkLinks(file, read(file))),
    ...checkGuide(),
  ]
}


function repoPath(arg) {
  return Path.relative(REPO, Path.resolve(REPO, arg)).split(Path.sep).join('/')
}


module.exports = {
  CONFIG, REPO, pages, markdownFiles, checkPage, checkLinks, checkGuide, checkAll,
  bannedList, prose, fenceless, logical, lineAt, firstSingular, slug, anchors, repoPath,
  CODE_SPAN, CODE_WRAP, EMOJI, EXCLAMATION, FIRST_WE, INTERNAL,
}


if (require.main === module) {
  const args = process.argv.slice(2)
  const only = args.filter((a) => !a.startsWith('--')).map(repoPath)

  if (args.includes('--list')) {
    process.stdout.write(pages().join('\n') + '\n')
    process.exit(0)
  }

  let findings
  try {
    findings = checkAll(0 < only.length ? only : undefined)
  }
  catch (err) {
    process.stderr.write(err.message + '\n')
    process.exit(2)
  }

  const limit = Number((args.find((a) => a.startsWith('--limit=')) || '--limit=40').slice(8))
  findings.slice(0, limit).forEach((f) => {
    process.stdout.write(`${f.file}:${f.line}: ${f.rule}: ${f.detail}\n`)
    if (f.excerpt) process.stdout.write(`    ${f.excerpt}\n`)
  })
  if (findings.length > limit) process.stdout.write(`... ${findings.length - limit} more\n`)
  const files = new Set(findings.map((f) => f.file)).size
  process.stdout.write(0 === findings.length
    ? 'prose gate: clean\n'
    : `prose gate: ${findings.length} finding(s) in ${files} file(s), STYLE-GUIDE.md\n`)
  process.exit(0 === findings.length ? 0 : 1)
}
