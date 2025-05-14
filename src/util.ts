import { Key, PercentLength } from './index.js';

export function percent(value: number): PercentLength {
  return {
    ty: 'percent',
    value,
  };
}

export function length(value: number): PercentLength {
  return {
    ty: 'length',
    value,
  };
}

export function key(code: number, extended: boolean = false): Key {
  return {
    code,
    extended,
  };
}
