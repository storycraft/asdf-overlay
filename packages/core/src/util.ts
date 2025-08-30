import { Key, PercentLength } from './index.js';

/**
 * Utility function to create `PercentLength` using percent relative value.
 */
export function percent(value: number): PercentLength {
  return {
    ty: 'percent',
    value,
  };
}

/**
 * Utilty function to create `PercentLength` using absolute length value.
 */
export function length(value: number): PercentLength {
  return {
    ty: 'length',
    value,
  };
}

/**
 * Utility function to create `Key` using key code and optional extended flag.
 */
export function key(code: number, extended: boolean = false): Key {
  return {
    code,
    extended,
  };
}
