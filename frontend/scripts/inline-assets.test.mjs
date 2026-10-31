import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { Script } from 'node:vm';
import { inlineUiAssets } from './inline-assets.mjs';

async function fixture(callback, options = {}) {
  const dir = await mkdtemp(join(tmpdir(), 'kira-inline-test-'));
  try {
    await mkdir(join(dir, 'assets'));
    await writeFile(join(dir, 'index.html'),
      '<!doctype html><html><head>' +
      '<link rel="stylesheet" crossorigin href="./assets/main-A1.css">' +
      '</head><body><div id="app"></div>' +
      '<script type="module" crossorigin src="./assets/main-A1.js"></script>' +
      '</body></html>');
    await writeFile(join(dir, 'assets/main-A1.js'), options.js ?? 'window.kiraReady = true;');
    await writeFile(join(dir, 'assets/main-A1.css'), options.css ?? '.bar{color:white}');
    await callback(dir);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

test('inlines Vite bundles without leaving module subrequests', async () => {
  await fixture(async (dir) => {
    const counts = await inlineUiAssets(dir);
    const html = await readFile(join(dir, 'index.html'), 'utf8');
    assert.ok(counts.scriptBytes > 0);
    assert.ok(counts.cssBytes > 0);
    assert.match(html, /<script>\s*\(function\(\)/);
    assert.match(html, /<style>\s*\.bar/);
    assert.doesNotMatch(html, /type="module"|src="\.\/assets|href="\.\/assets/);
    const bundled = /<script>\n([\s\S]*?)\n<\/script>/.exec(html)?.[1];
    assert.ok(bundled);
    new Script(bundled);
  });
});

test('preserves dollar replacement tokens in JS and CSS instead of splicing HTML into scripts', async () => {
  const tokens = ['test('allows HTML-like text inside bundled JS without treating it as an external asset', async () => {', "
  await fixture(async (dir) => {
    const result = await inlineUiAssets(dir);
    assert.ok(result.scriptBytes > 0);
    const html = await readFile(join(dir, 'index.html'), 'utf8');
    assert.match(html, /window\.kiraMarkup/);
  }, { js: 'window.kiraMarkup = \'<link rel="stylesheet" href="https://example.invalid/a.css">\';' });
});

test('rejects an output that cannot run as a classic script', async () => {
  await fixture(async (dir) => {
    await assert.rejects(inlineUiAssets(dir), /Cannot use import statement|Unexpected token/);
  }, { js: 'import {x} from "./other.js";' });
});

test('rejects attempts to leave the output directory', async () => {
  await fixture(async (dir) => {
    const htmlPath = join(dir, 'index.html');
    const html = await readFile(htmlPath, 'utf8');
    await writeFile(htmlPath, html.replace('./assets/main-A1.js', '../outside.js'));
    await assert.rejects(inlineUiAssets(dir), /Unexpected js asset URL/);
  });
});

test('rejects unsafe HTML terminators in bundled assets', async () => {
  await fixture(async (dir) => {
    await assert.rejects(inlineUiAssets(dir), /closing script tag/);
  }, { js: 'console.log("</script>");' });
});

test('refuses to silently accept multiple JS chunks', async () => {
  await fixture(async (dir) => {
    const htmlPath = join(dir, 'index.html');
    const html = await readFile(htmlPath, 'utf8');
    await writeFile(htmlPath, html.replace('</body>',
      '<script type="module" src="./assets/other.js"></script></body>'));
    await assert.rejects(inlineUiAssets(dir), /exactly one external Vite JS entrypoint/);
  });
});
", 'import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { Script } from 'node:vm';
import { inlineUiAssets } from './inline-assets.mjs';

async function fixture(callback, options = {}) {
  const dir = await mkdtemp(join(tmpdir(), 'kira-inline-test-'));
  try {
    await mkdir(join(dir, 'assets'));
    await writeFile(join(dir, 'index.html'),
      '<!doctype html><html><head>' +
      '<link rel="stylesheet" crossorigin href="./assets/main-A1.css">' +
      '</head><body><div id="app"></div>' +
      '<script type="module" crossorigin src="./assets/main-A1.js"></script>' +
      '</body></html>');
    await writeFile(join(dir, 'assets/main-A1.js'), options.js ?? 'window.kiraReady = true;');
    await writeFile(join(dir, 'assets/main-A1.css'), options.css ?? '.bar{color:white}');
    await callback(dir);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

test('inlines Vite bundles without leaving module subrequests', async () => {
  await fixture(async (dir) => {
    const counts = await inlineUiAssets(dir);
    const html = await readFile(join(dir, 'index.html'), 'utf8');
    assert.ok(counts.scriptBytes > 0);
    assert.ok(counts.cssBytes > 0);
    assert.match(html, /<script>\s*\(function\(\)/);
    assert.match(html, /<style>\s*\.bar/);
    assert.doesNotMatch(html, /type="module"|src="\.\/assets|href="\.\/assets/);
    const bundled = /<script>\n([\s\S]*?)\n<\/script>/.exec(html)?.[1];
    assert.ok(bundled);
    new Script(bundled);
  });
});

', '$'];
  const js = 'window.kiraTokens = ' + JSON.stringify(tokens) + ';';
  const css = ':root::before { content: ' + JSON.stringify(tokens.join('|')) + '; }';
  await fixture(async (dir) => {
    await inlineUiAssets(dir);
    const html = await readFile(join(dir, 'index.html'), 'utf8');

    // The emitted script and style must contain EXACTLY the original bytes.
    // String.replace with a string replacement silently corrupts the $ tokens.
    const emittedJs = /<script>\n\(function\(\)\{\n([\s\S]*?)\n\}\)\(\);\n<\/script>/.exec(html)?.[1];
    const emittedCss = /<style>\n([\s\S]*?)\n<\/style>/.exec(html)?.[1];
    assert.equal(emittedJs, js);
    assert.equal(emittedCss, css);
    assert.equal((html.match(/<\/script>/g) ?? []).length, 1);
    assert.equal((html.match(/<\/style>/g) ?? []).length, 1);
    new Script(emittedJs);
    assert.doesNotMatch(html, /<script src=|<link rel="stylesheet"/);
  }, { js, css });
});

test('allows HTML-like text inside bundled JS without treating it as an external asset', async () => {
  await fixture(async (dir) => {
    const result = await inlineUiAssets(dir);
    assert.ok(result.scriptBytes > 0);
    const html = await readFile(join(dir, 'index.html'), 'utf8');
    assert.match(html, /window\.kiraMarkup/);
  }, { js: 'window.kiraMarkup = \'<link rel="stylesheet" href="https://example.invalid/a.css">\';' });
});

test('rejects an output that cannot run as a classic script', async () => {
  await fixture(async (dir) => {
    await assert.rejects(inlineUiAssets(dir), /Cannot use import statement|Unexpected token/);
  }, { js: 'import {x} from "./other.js";' });
});

test('rejects attempts to leave the output directory', async () => {
  await fixture(async (dir) => {
    const htmlPath = join(dir, 'index.html');
    const html = await readFile(htmlPath, 'utf8');
    await writeFile(htmlPath, html.replace('./assets/main-A1.js', '../outside.js'));
    await assert.rejects(inlineUiAssets(dir), /Unexpected js asset URL/);
  });
});

test('rejects unsafe HTML terminators in bundled assets', async () => {
  await fixture(async (dir) => {
    await assert.rejects(inlineUiAssets(dir), /closing script tag/);
  }, { js: 'console.log("</script>");' });
});

test('refuses to silently accept multiple JS chunks', async () => {
  await fixture(async (dir) => {
    const htmlPath = join(dir, 'index.html');
    const html = await readFile(htmlPath, 'utf8');
    await writeFile(htmlPath, html.replace('</body>',
      '<script type="module" src="./assets/other.js"></script></body>'));
    await assert.rejects(inlineUiAssets(dir), /exactly one external Vite JS entrypoint/);
  });
});
