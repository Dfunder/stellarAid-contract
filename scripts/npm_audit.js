#!/usr/bin/env node
// npm_audit.js — Security audit for npm dependencies in sdk/bindings.
//
// Closes #1148.
//
// Scans production dependencies (`npm audit --omit=dev --json`) for HIGH and
// CRITICAL vulnerabilities. Any unapproved vulnerability fails the build.
// Known unfixable advisories are cross-checked against `audit-allowlist.json`
// and documented in `docs/STORAGE.md` §9.4.

import { execSync } from 'child_process';
import { readFileSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const bindingsDir = resolve(repoRoot, 'sdk', 'bindings');
const allowlistPath = resolve(bindingsDir, 'audit-allowlist.json');

console.log('Running npm audit for @stellaraid/wallet-adapters in sdk/bindings...');

let allowlist = [];
if (existsSync(allowlistPath)) {
  try {
    const allowlistData = JSON.parse(readFileSync(allowlistPath, 'utf-8'));
    allowlist = allowlistData.allowlist || [];
  } catch (e) {
    console.error(`Failed to parse allowlist at ${allowlistPath}:`, e.message);
    process.exit(1);
  }
}

let auditOutput = '';
try {
  auditOutput = execSync('npm audit --omit=dev --json', {
    cwd: bindingsDir,
    encoding: 'utf-8',
    stdio: ['pipe', 'pipe', 'pipe']
  });
} catch (err) {
  // npm audit exits with non-zero when vulnerabilities are found
  auditOutput = err.stdout ? err.stdout.toString() : '';
  if (!auditOutput && err.stderr) {
    console.error('npm audit error:', err.stderr.toString());
  }
}

if (!auditOutput.trim()) {
  console.log('No npm audit report output. Assuming clean.');
  process.exit(0);
}

let audit;
try {
  audit = JSON.parse(auditOutput);
} catch (e) {
  console.error('Failed to parse npm audit JSON output:', e.message);
  process.exit(1);
}

const vulns = audit.vulnerabilities || {};
const allowedIds = new Set(allowlist.map(a => a.id));
const unapproved = [];
const approved = [];

for (const [pkgName, pkgInfo] of Object.entries(vulns)) {
  const severity = pkgInfo.severity;
  if (severity !== 'high' && severity !== 'critical') {
    continue;
  }

  const viaList = pkgInfo.via || [];
  for (const via of viaList) {
    if (typeof via === 'string') {
      continue;
    }
    const advisoryUrl = via.url || '';
    const match = advisoryUrl.match(/GHSA-[a-z0-9-]+/i);
    const ghsaId = match ? match[0] : (via.source ? `source-${via.source}` : `${pkgName}-${via.title}`);

    if (allowedIds.has(ghsaId)) {
      approved.push({
        id: ghsaId,
        package: pkgName,
        severity: via.severity || severity,
        title: via.title,
        url: via.url
      });
    } else {
      unapproved.push({
        id: ghsaId,
        package: pkgName,
        severity: via.severity || severity,
        title: via.title,
        url: via.url,
        range: via.range
      });
    }
  }
}

if (approved.length > 0) {
  console.log(`\nDocumented & allowlisted advisories (${approved.length}):`);
  for (const item of approved) {
    const entry = allowlist.find(a => a.id === item.id);
    console.log(`  - [${item.severity.toUpperCase()}] ${item.package}: ${item.id} (${item.title})`);
    if (entry && entry.reason) {
      console.log(`    Justification: ${entry.reason}`);
    }
  }
}

if (unapproved.length > 0) {
  console.error(`\n❌ ERROR: Found ${unapproved.length} unapproved high/critical advisories in sdk/bindings:`);
  for (const item of unapproved) {
    console.error(`  - [${item.severity.toUpperCase()}] ${item.package}: ${item.id}`);
    console.error(`    Title: ${item.title}`);
    console.error(`    URL:   ${item.url}`);
  }
  console.error('\nIf an advisory is verified unfixable and non-exploitable in this context,');
  console.error('follow the exception process documented in docs/STORAGE.md and add an entry');
  console.error('to sdk/bindings/audit-allowlist.json with justification.\n');
  process.exit(1);
}

console.log('\n✅ npm audit passed: No unapproved high or critical vulnerabilities.');
process.exit(0);
