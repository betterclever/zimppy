/**
 * zimppy UI primitives — consistent terminal output across all CLIs.
 *
 * Usage:
 *   ui.ok('Synced')              // ✓ Synced
 *   ui.fail('Connection lost')   // ✗ Connection lost
 *   ui.info('Spendable', '1.2 ZEC')  // Spendable: 1.2 ZEC
 *   ui.dim('txid: abc123')       // txid: abc123 (dimmed)
 *   ui.heading('Wallet')         // --- Wallet ---
 *   const sp = ui.spinner('Syncing...')
 *   sp.update('new message')
 *   sp.ok('Synced')
 *   sp.fail('Timeout')
 */

const FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

// ── ANSI helpers ─────────────────────────────────────────────────

const c = {
  reset:   '\x1b[0m',
  bold:    '\x1b[1m',
  dim:     '\x1b[2m',
  cyan:    '\x1b[36m',
  green:   '\x1b[32m',
  red:     '\x1b[31m',
  yellow:  '\x1b[33m',
  clearLn: '\r\x1b[K',
} as const

function styled(color: string, text: string, bold = false): string {
  return `${bold ? c.bold : ''}${color}${text}${c.reset}`
}

// ── Public API ───────────────────────────────────────────────────

/** ✓ message */
function ok(msg: string): void {
  process.stderr.write(`  ${styled(c.green, '✓', true)} ${msg}\n`)
}

/** ✗ message */
function fail(msg: string): void {
  process.stderr.write(`  ${styled(c.red, '✗', true)} ${msg}\n`)
}

/** Label: value (value in cyan) */
function info(label: string, value: string): void {
  process.stderr.write(`  ${label}: ${styled(c.cyan, value)}\n`)
}

/** Dimmed text */
function dim(msg: string): void {
  process.stderr.write(`  ${styled(c.dim, msg)}\n`)
}

/** Warning text (yellow) */
function warn(msg: string): void {
  process.stderr.write(`  ${styled(c.yellow, msg)}\n`)
}

/** --- Heading --- */
function heading(title: string): void {
  process.stderr.write(`\n  --- ${title} ---\n`)
}

/** Plain stderr line with 2-space indent */
function line(msg: string): void {
  process.stderr.write(`  ${msg}\n`)
}

/** Format zatoshis as "X zat (Y.ZZZZ ZEC)" */
function zat(amount: number | string): string {
  const n = typeof amount === 'string' ? Number(amount) : amount
  const zec = n / 100_000_000
  if (zec >= 0.01) return `${n} zat (${zec.toFixed(4)} ZEC)`
  return `${n} zat`
}

/** Format an address for display (truncated) */
function addr(address: string): string {
  if (address.length <= 40) return address
  return `${address.slice(0, 20)}...${address.slice(-12)}`
}

// ── Spinner ──────────────────────────────────────────────────────

interface Spinner {
  /** Update the spinner message (renders on next tick) */
  update(msg: string): void
  /** Stop with green ✓ */
  ok(msg?: string): void
  /** Stop with red ✗ and detail line */
  fail(msg: string, detail?: string): void
  /** Stop with custom message (no icon) */
  stop(): void
}

function spinner(label: string): Spinner {
  const start = Date.now()
  let frame = 0
  let message = label

  const render = () => {
    const elapsed = Math.round((Date.now() - start) / 1000)
    process.stderr.write(
      `${c.clearLn}  ${styled(c.cyan, FRAMES[frame])} ${message} ${styled(c.dim, `${elapsed}s`)}`,
    )
    frame = (frame + 1) % FRAMES.length
  }

  render()
  const timer = setInterval(render, 80)

  const cleanup = () => {
    clearInterval(timer)
    process.stderr.write(c.clearLn)
  }

  return {
    update(msg: string) {
      message = msg
    },
    ok(msg?: string) {
      cleanup()
      const elapsed = Math.round((Date.now() - start) / 1000)
      process.stderr.write(
        `  ${styled(c.green, '✓', true)} ${msg ?? label} ${styled(c.dim, `${elapsed}s`)}\n`,
      )
    },
    fail(msg: string, detail?: string) {
      cleanup()
      const elapsed = Math.round((Date.now() - start) / 1000)
      process.stderr.write(
        `  ${styled(c.red, '✗', true)} ${msg} ${styled(c.dim, `${elapsed}s`)}\n`,
      )
      if (detail) process.stderr.write(`    ${detail}\n`)
    },
    stop() {
      cleanup()
    },
  }
}

export const ui = {
  ok,
  fail,
  info,
  dim,
  warn,
  heading,
  line,
  zat,
  addr,
  spinner,
} as const
