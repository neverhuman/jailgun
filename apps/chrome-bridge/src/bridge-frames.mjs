export const MAX_LINE_BYTES = 1024 * 1024;

/** Bound a frame before decoding it, including when the sender never sends LF/EOF. */
export async function* readFrames(input) {
  const frame = Buffer.alloc(MAX_LINE_BYTES + 1); // One optional trailing CR.
  const decoder = new TextDecoder('utf-8', { fatal: true });
  let used = 0;
  for await (const chunk of input) {
    let offset = 0;
    while (offset < chunk.length) {
      const newline = chunk.indexOf(10, offset);
      const end = newline < 0 ? chunk.length : newline;
      const size = used + end - offset;
      const last = end > offset ? chunk[end - 1] : frame[used - 1];
      if (size > MAX_LINE_BYTES + Number(last === 13)) throw new Error('bridge-frame-too-large');
      chunk.copy(frame, used, offset, end);
      used = size;
      offset = end + Number(newline >= 0);
      if (newline >= 0) {
        yield decode(frame.subarray(0, used - Number(frame[used - 1] === 13)), decoder);
        used = 0;
      }
    }
  }
  if (used) yield decode(frame.subarray(0, used - Number(frame[used - 1] === 13)), decoder);
}

function decode(bytes, decoder) {
  try { return decoder.decode(bytes); }
  catch { throw new Error('bridge-frame-invalid-utf8'); }
}
