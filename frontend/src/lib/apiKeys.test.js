import test from 'node:test';
import assert from 'node:assert/strict';
import { maskApiKey, apiKeyCreatedMessage } from './apiKeys.js';

test('maskApiKey keeps prefix and suffix only', () => {
  assert.equal(maskApiKey('tdue_abcdefghijklmnopqrstuvwxyz'), 'tdue_ab…wxyz');
});

test('maskApiKey handles missing values', () => {
  assert.equal(maskApiKey(''), '');
});

test('apiKeyCreatedMessage reminds users the key is one-time visible', () => {
  assert.equal(
    apiKeyCreatedMessage('Claude Desktop'),
    'API key “Claude Desktop” created. Copy it now — it will only be shown once.'
  );
});
