#!/usr/bin/env node
/**
 * Inline the single production Vite JS/CSS bundles into index.html.
 *
 * WebKitGTK's private URI scheme loads HTML, but ES module subrequests can
 * fail with opaque "Script error." on certain WebKit versions. A classic,
 * self-contained script avoids module fetching and reports meaningful errors.
 * Fails closed when Vite emits an incompatible/multi-chunk bundle.
 */
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { Script } from 'node:vm';
import { fileURLToPath } from 'node:url';

const SCRIPT_TAG = /<script\b(?=[^>]*\bsrc\s*=)[^>]*>\s*<\/script>/gi;
const LINK_TAG = /<link\b[^>]*>/gi;

function attribute(tag, name) {
  const pattern = new RegExp("(?:^|\\s)" + name + "\\s*=\\s*(['\"])(.*?)\\1", "i");
  return pattern.exec(tag)?.[2] ?? null;
}

function assetName(reference, kind) {
  // Vite base: './' yields ./assets/<hash>.js / .css.
  // No external URLs, absolute paths, parent traversal or URL fragments.
  const pattern = kind === 'js'
    ? /^\.\/assets\/[A-Za-z0-9_.-]+\.js$/
    : /^\.\/assets\/[A-Za-z0-9_.-]+\.css$/;
  if (!pattern.test(reference)) {
    throw new Error('Unexpected ' + kind + ' asset URL: ' + reference);
  }
  return reference.slice(2);
}

export async function inlineUiAssets(outputDirectory) {
  const directory = resolve(outputDirectory);
  const htmlPath = resolve(directory, 'index.html');
  let html = await readFile(htmlPath, 'utf8');

  const scripts = [...html.matchAll(SCRIPT_TAG)];
  if (scripts.length !== 1) {
    throw new Error('Expected exactly one external Vite JS entrypoint, found ' + scripts.length);
  }

  const scriptTag = scripts[0][0];
  const jsReference = attribute(scriptTag, 'src');
  if (!jsReference) throw new Error('Vite entrypoint has no src');
  const js = await readFile(resolve(directory, assetName(jsReference, 'js')), 'utf8');

  if (/<\/script\b/i.test(js)) {
    throw new Error('Bundled JavaScript includes a closing script tag');
  }
  // A classic script does not use opaque module origins. Do not silently
  // inline a bundle that contains ESM-only syntax.
  new Script('(function(){\n' + js + '\n})();', { filename: jsReference });
  html = html.replace(scriptTag, '<script>\n(function(){\n' + js + '\n})();\n</script>');

  const styles = [...html.matchAll(LINK_TAG)]
    .map((match) => match[0])
    .filter((tag) => attribute(tag, 'rel')?.toLowerCase() === 'stylesheet');
  if (styles.length !== 1) {
    throw new Error('Expected exactly one Vite CSS stylesheet, found ' + styles.length);
  }

  const cssReference = attribute(styles[0], 'href');
  if (!cssReference) throw new Error('Vite stylesheet has no href');
  const css = await readFile(resolve(directory, assetName(cssReference, 'css')), 'utf8');
  if (/<\/style\b/i.test(css)) {
    throw new Error('Bundled CSS includes a closing style tag');
  }
  html = html.replace(styles[0], '<style>\n' + css + '\n</style>');

  if (SCRIPT_TAG.test(html) || [...html.matchAll(LINK_TAG)]
    .some((match) => attribute(match[0], 'rel')?.toLowerCase() === 'stylesheet')) {
    throw new Error('Unexpected remaining external UI assets');
  }

  await writeFile(htmlPath, html);
  return { scriptBytes: Buffer.byteLength(js), cssBytes: Buffer.byteLength(css) };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = process.argv[2] ?? 'dist';
  inlineUiAssets(path)
    .then(({ scriptBytes, cssBytes }) => {
      console.log('Kira UI: inlined JS (' + scriptBytes + ' B) and CSS (' + cssBytes + ' B)');
    })
    .catch((error) => {
      console.error('Kira UI inlining failed:', error.message);
      process.exitCode = 1;
    });
}
