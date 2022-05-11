let process = require('process');
let Stream = require('stream');
let mdeps = require('module-deps');

const getBuiltins = function () {
	// Making this function memoized ensures the set of builtins is
	// only generated once, and can then be reused on each call to builtins()
	// in the main function below.
	let cached = null;

	return () => {
		if (cached !== null) {
			return cached;
		}

		// This is the list of built-in packages provided by Node.js.
		const pkgs = [
			'assert',
			'async_hooks',
			'buffer',
			'child_process',
			'cluster',
			'console',
			'constants',
			'crypto',
			'dgram',
			'dns',
			'domain',
			'events',
			'fs',
			'http',
			'http2',
			'https',
			'inspector',
			'module',
			'net',
			'os',
			'path',
			'perf_hooks',
			'process',
			'punycode',
			'querystring',
			'readline',
			'repl',
			'stream',
			'string_decoder',
			'sys',
			'timers',
			'tls',
			'trace_events',
			'tty',
			'url',
			'util',
			'v8',
			'vm',
			'wasi',
			'worker_threads',
			'zlib'
		];

		let builtins = new Set();

		for (const pkg of pkgs) {
			builtins.add(pkg);
		}

		cached = builtins;

		return builtins;
	}
};

const processArgs = function () {
	let args = process.argv.slice(2);

	if (args.length < 1) {
		process.stderr.write("error: missing entrypoint name");
		process.exit(1);
	} else if (args.length > 1) {
		process.stderr.write("error: only one entrypoint accepted");
		process.exit(1);
	}

	return args;
};

/*
 * This function is adapted from the 'JSONStream' NPM library.
 *
 * Copyright (c) 2011 Dominic Tarr
 *
 * Permission is hereby granted, free of charge,
 * to any person obtaining a copy of this software and
 * associated documentation files (the "Software"), to
 * deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify,
 * merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom
 * the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice
 * shall be included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
 * OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR
 * ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
const stringify = function (op, sep, cl, indent) {
  indent = indent || 0;

  if (op === false) {
    op = '';
    sep = '\n';
    cl = '';
  } else if (op == null) {
    op = '[\n';
    sep = '\n,\n';
    cl = '\n]\n';
  }

  var stream;
  var first = true;
  var anyData = false;

  var write = function (data) {
    anyData = true;

    try {
      var json = JSON.stringify(data, null, indent);
    } catch (err) {
      return stream.emit('error', err);
    }

    if (first) {
        first = false;
        stream.queue(op + json);
    } else {
        stream.queue(sep + json);
    }
  };

  var end = function (_data) {
    if (!anyData) {
      stream.queue(op)
    }

    stream.queue(cl)
    stream.queue(null)
  };

  stream = through(write, end);
  return stream;
};

/*
 * This function is adapted from the 'through' NPM library.
 *
 * Copyright (c) 2011 Dominic Tarr
 *
 * Permission is hereby granted, free of charge,
 * to any person obtaining a copy of this software and
 * associated documentation files (the "Software"), to
 * deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify,
 * merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom
 * the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice
 * shall be included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
 * OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR
 * ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
const through = function (write, end, opts) {
  write = write || function (data) { this.queue(data) };
  end = end || function () { this.queue(null) };

  var ended = false;
  var destroyed = false;
  var buffer = [];
  var _ended = false;

  var stream = new Stream();
  stream.readable = stream.writable = true;
  stream.paused = false;
  stream.autoDestroy = !(opts && opts.autoDestroy === false);

  stream.write = function (data) {
    write.call(this, data);
    return !stream.paused;
  };

  function drain() {
    while (buffer.length && !stream.paused) {
      var data = buffer.shift();

      if (null === data) {
        return stream.emit('end');
      } else {
        stream.emit('data', data);
      }
    }
  }

  stream.queue = stream.push = function (data) {
    if (_ended) {
        return stream;
    }

    if (data === null) {
        _ended = true;
    }

    buffer.push(data);
    drain();
    return stream;
  };

  stream.on('end', function () {
    stream.readable = false;

    if (!stream.writable && stream.autoDestroy)
      process.nextTick(function () {
        stream.destroy();
      });
  });

  function _end() {
    stream.writable = false;
    end.call(stream);

    if (!stream.readable && stream.autoDestroy) {
      stream.destroy();
    }
  }

  stream.end = function (data) {
    if (ended) {
        return;
    }

    ended = true

    if (arguments.length) {
        stream.write(data)
    }

    _end();
    return stream;
  };

  stream.destroy = function () {
    if (destroyed) {
        return;
    }

    destroyed = true;
    ended = true;
    buffer.length = 0;
    stream.writable = stream.readable = false;
    stream.emit('close');
    return stream;
  };

  stream.pause = function () {
    if (stream.paused) {
        return;
    }

    stream.paused = true;
    return stream;
  };

  stream.resume = function () {
    if (stream.paused) {
      stream.paused = false;
      stream.emit('resume');
    }

    drain()

    if (!stream.paused) {
      stream.emit('drain');
    }

    return stream;
  };

  return stream;
};

const main = function () {
	let args = processArgs();

	// Get a memoized function that returns the builtins.
	let builtins = getBuiltins();

	const filterBuiltins = function (name) {
		return !builtins().has(name);
	};

	let md = mdeps({ filter: filterBuiltins });
	md.pipe(stringify()).pipe(process.stdout);
	md.end({ file: args[0] });
};

main();

