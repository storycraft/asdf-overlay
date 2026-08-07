const { EventEmitter } = require('node:events');

// @ts-check
module.exports = require('./native');

function defaultDllDir() {
  return __dirname;
}
module.exports.defaultDllDir = defaultDllDir;

module.exports.Overlay.EventEmitter = EventEmitter;
