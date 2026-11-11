// Load the gateway protocol (and Effect Schema) and the Durable Object classes
// before any test runs. Otherwise the first test in each file that touches a
// Durable Object pays for loading the whole room module graph (about 1.3 s on
// an idle machine, past 5 s when CI's four cores run several files at once)
// inside its test timeout. Setup files are not subject to the test timeout.
import "../src/lib/gateway-protocol";
import "../realtime-test-entry";

const noop = () => {};

console.debug = noop;
console.info = noop;
console.log = noop;
console.warn = noop;
console.error = noop;
