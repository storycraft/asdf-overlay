import { PercentLength } from './addon';

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
