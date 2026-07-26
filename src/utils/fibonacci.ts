const MAX_FIBONACCI_INDEX = 78;

/** Returns the Fibonacci number at a zero-based index. */
export function fibonacci(index: number): number {
  if (!Number.isSafeInteger(index) || index < 0 || index > MAX_FIBONACCI_INDEX) {
    throw new RangeError(`index must be a safe integer between 0 and ${MAX_FIBONACCI_INDEX}`);
  }

  let previous = 0;
  let current = 1;

  for (let position = 0; position < index; position += 1) {
    [previous, current] = [current, previous + current];
  }

  return previous;
}
