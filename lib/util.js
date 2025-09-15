export function percent(value) {
    return {
        ty: 'percent',
        value,
    };
}
export function length(value) {
    return {
        ty: 'length',
        value,
    };
}
export function key(code, extended = false) {
    return {
        code,
        extended,
    };
}
