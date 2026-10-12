// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// A small JSON Schema validator for the keywords that spec/rqp/schemas use.
// It reads the schema files themselves, so the patterns and ranges are never
// copied. A keyword that it does not know is an error (as Ajv strict mode), so a
// later schema change cannot be ignored without notice. The tests compare it
// with the example instances of spec/rqp/examples (each invalid instance names
// the keyword and the instance path that Ajv 8.20.0 reports).

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const ANNOTATIONS = new Set(['$schema', '$id', 'title', 'description', 'default', '$defs']);
const KNOWN = new Set([
  ...ANNOTATIONS,
  'type', 'const', 'enum', 'pattern', 'minLength', 'maxLength', 'minimum', 'maximum',
  'minItems', 'maxItems', 'items', 'properties', 'required', 'additionalProperties', '$ref',
]);

export class SchemaError extends Error {
  constructor(message) {
    super(message);
    this.name = 'SchemaError';
    this.code = 'RQP_SCHEMA_UNSUPPORTED';
  }
}

const SCHEMA_DIR = new URL('../../../spec/rqp/schemas/', import.meta.url);

/** Load spec/rqp/schemas/<kind>.schema.json. */
export function loadSchema(kind) {
  return JSON.parse(readFileSync(fileURLToPath(new URL(`${kind}.schema.json`, SCHEMA_DIR)), 'utf8'));
}

function typeOk(type, v) {
  switch (type) {
    case 'object':
      return v !== null && typeof v === 'object' && !Array.isArray(v);
    case 'array':
      return Array.isArray(v);
    case 'string':
      return typeof v === 'string';
    case 'integer':
      return Number.isInteger(v);
    case 'number':
      return typeof v === 'number' && Number.isFinite(v);
    case 'boolean':
      return typeof v === 'boolean';
    case 'null':
      return v === null;
    default:
      throw new SchemaError(`unknown type ${type}`);
  }
}

/** Number of Unicode code points (JSON Schema string length). */
function codePoints(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      const d = s.charCodeAt(i + 1);
      if (d >= 0xdc00 && d <= 0xdfff) i++;
    }
    n++;
  }
  return n;
}

/**
 * Validate a value. Returns [] when it is valid, else a list of
 * { keyword, instancePath } (all errors, as Ajv with allErrors).
 */
export function validate(root, value) {
  const errors = [];
  const resolve = (ref) => {
    const m = /^#\/\$defs\/([A-Za-z0-9_]+)$/.exec(ref);
    if (m === null || root.$defs?.[m[1]] === undefined) throw new SchemaError(`unsupported $ref ${ref}`);
    return root.$defs[m[1]];
  };
  const visit = (schema, v, path) => {
    for (const k of Object.keys(schema)) if (!KNOWN.has(k)) throw new SchemaError(`unsupported keyword ${k}`);
    if (schema.additionalProperties !== undefined && schema.additionalProperties !== false) {
      throw new SchemaError('additionalProperties other than false is not supported');
    }
    const err = (keyword) => errors.push({ keyword, instancePath: path });
    if (schema.$ref !== undefined) visit(resolve(schema.$ref), v, path);
    if (schema.type !== undefined && !typeOk(schema.type, v)) {
      err('type');
      return;
    }
    if (schema.const !== undefined && v !== schema.const) err('const');
    if (schema.enum !== undefined && !schema.enum.includes(v)) err('enum');
    if (typeof v === 'string') {
      if (schema.minLength !== undefined && codePoints(v) < schema.minLength) err('minLength');
      if (schema.maxLength !== undefined && codePoints(v) > schema.maxLength) err('maxLength');
      if (schema.pattern !== undefined && !new RegExp(schema.pattern, 'u').test(v)) err('pattern');
    }
    if (typeof v === 'number') {
      if (schema.minimum !== undefined && v < schema.minimum) err('minimum');
      if (schema.maximum !== undefined && v > schema.maximum) err('maximum');
    }
    if (Array.isArray(v)) {
      if (schema.minItems !== undefined && v.length < schema.minItems) err('minItems');
      if (schema.maxItems !== undefined && v.length > schema.maxItems) err('maxItems');
      if (schema.items !== undefined) v.forEach((item, i) => visit(schema.items, item, `${path}/${i}`));
    }
    if (typeOk('object', v)) {
      const props = schema.properties ?? {};
      for (const name of schema.required ?? []) if (!Object.hasOwn(v, name)) err('required');
      for (const [name, member] of Object.entries(v)) {
        const pointer = `${path}/${name.replaceAll('~', '~0').replaceAll('/', '~1')}`;
        if (Object.hasOwn(props, name)) visit(props[name], member, pointer);
        else if (schema.additionalProperties === false) err('additionalProperties');
      }
    }
  };
  visit(root, value, '');
  return errors;
}
