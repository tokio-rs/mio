/* No implementation yet.
 *
 * The goal is to normalize window's IOCP API to the various *NIX's readiness
 * model. This strategy will require maintaining a slab of buffers that will be
 * used to hold data as it is in-flight, allowing the user's buffer to remain
 * reusable.
 */
