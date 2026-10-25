import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../../src/windows_policy.rs', import.meta.url), 'utf8');
const bridge = source.match(/const BRIDGE: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(bridge, 'the exact document-created bridge must be available for execution');

function host(isTop = true) {
  const messages = [], listeners = new Map();
  const window = {
    chrome: {webview: {postMessage(message) { messages.push(message); }}},
    addEventListener(name, handler) { listeners.set(name, handler); },
    localStorage: {old: true}, indexedDB: {old: true},
    AudioContext: class {}, RTCPeerConnection: class {},
    open() { throw new Error('new windows must be disabled'); },
  };
  window.top = isTop ? window : {};
  vm.runInNewContext(bridge, {window});
  return {window, messages, listeners};
}

test('WebView2 bridge forwards page objects without string encoding', () => {
  const {window, messages} = host();
  for (const message of [
    {type: 'ready'}, {type: 'open-commit', hash: 'abc'},
    {type: 'open-file', path: 'src/a b.rs', line: 2},
    {type: 'painted', generation: 7},
  ]) {
    window.webkit.messageHandlers.impro.postMessage(message);
    assert.equal(messages.at(-1), message, 'the message object must reach WebMessageAsJson directly');
  }
  assert.ok(Object.isFrozen(window.webkit.messageHandlers.impro));
  assert.equal(window.localStorage, undefined);
  assert.equal(window.indexedDB, undefined);
  assert.equal(window.AudioContext, undefined);
  assert.equal(window.RTCPeerConnection, undefined);
  assert.equal(window.open(), null);
});

test('WebView2 bridge forwards uncaught exceptions and promise rejection details', () => {
  const {messages, listeners} = host();
  listeners.get('error')({message: 'page failed'});
  listeners.get('unhandledrejection')({reason: 'worker failed'});
  assert.equal(messages[0].type, 'error');
  assert.equal(messages[0].message, 'page failed');
  assert.equal(messages[1].type, 'error');
  assert.equal(messages[1].message, 'worker failed');
});

test('WebView2 bridge is absent from subframes', () => {
  const {window, listeners} = host(false);
  assert.equal(window.webkit, undefined);
  assert.equal(listeners.size, 0);
});
