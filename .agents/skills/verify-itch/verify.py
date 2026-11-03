#!/usr/bin/env python3
"""Drive only a simulator launched by this process; retain wire evidence."""
import argparse
import binascii
from contextlib import contextmanager
import json
from pathlib import Path
import queue
import socket
import struct
import subprocess
import threading

ROOT = Path(__file__).resolve().parents[3]


def check(condition, message):
    if not condition:
        raise RuntimeError(message)


def text(value):
    data = value.encode('utf-8')
    return bytes([len(data)]) + data


def frame(payload):
    raw = struct.pack('<H', len(payload)) + payload
    raw += struct.pack('<H', binascii.crc_hqx(raw, 0xffff))
    encoded = bytearray([0])
    code_at = 0
    code = 1
    for byte in raw:
        if byte:
            encoded.append(byte)
            code += 1
        if not byte or code == 255:
            encoded[code_at] = code
            code_at = len(encoded)
            encoded.append(0)
            code = 1
    if code == 1 and raw[-1] and code_at:
        encoded.pop()
    else:
        encoded[code_at] = code
    return b'\0' + encoded + b'\0'


def receive(stream):
    encoded = bytearray()
    while True:
        byte = stream.recv(1)
        check(byte, 'simulator closed before replying')
        if byte == b'\0':
            if encoded:
                break
        else:
            encoded.extend(byte)
        check(len(encoded) <= 1034, 'oversized response frame')
    raw = bytearray()
    pos = 0
    while pos < len(encoded):
        code = encoded[pos]
        pos += 1
        check(pos + code - 1 <= len(encoded), 'truncated COBS response')
        raw.extend(encoded[pos:pos + code - 1])
        pos += code - 1
        if code < 255 and pos < len(encoded):
            raw.append(0)
    check(len(raw) >= 4, 'short frame')
    check(struct.unpack('<H', raw[:2])[0] == len(raw) - 4, 'bad frame length')
    check(binascii.crc_hqx(raw[:-2], 0xffff) == struct.unpack('<H', raw[-2:])[0], 'bad CRC')
    return bytes(raw[2:-2])


class Reader:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, count):
        result = self.data[self.pos:self.pos + count]
        check(len(result) == count, 'short response body')
        self.pos += count
        return result

    def number(self, fmt):
        return struct.unpack(fmt, self.take(struct.calcsize(fmt)))[0]

    def text(self):
        return self.take(self.number('<B')).decode('utf-8')

    def end(self):
        check(self.pos == len(self.data), 'trailing response bytes')


class Driver:
    def __init__(self, address, transcript):
        self.address = address
        self.transcript = transcript
        self.request_id = 0

    @contextmanager
    def connect(self):
        with socket.create_connection(self.address, timeout=3) as stream:
            yield stream
            # Await closure so the next journey cannot race the four-slot limit.
            stream.shutdown(socket.SHUT_WR)
            check(stream.recv(1) == b'', 'unexpected bytes while closing connection')

    def call(self, stream, kind, body=b'', status=0):
        self.request_id += 1
        payload = bytes([kind]) + struct.pack('<I', self.request_id) + body
        self.transcript.write(json.dumps({'action': 'send', 'type': kind,
                                         'request_id': self.request_id, 'payload': payload.hex()}) + '\n')
        self.transcript.flush()
        stream.sendall(frame(payload))
        reply = receive(stream)
        self.transcript.write(json.dumps({'action': 'receive', 'payload': reply.hex()}) + '\n')
        self.transcript.flush()
        check(reply[:5] == bytes([kind | 128]) + struct.pack('<I', self.request_id),
              'response type or request ID mismatch')
        check(len(reply) >= 6 and reply[5] == status, f'unexpected response: {reply.hex()}')
        return reply[6:]

    def hello(self, stream):
        body = Reader(self.call(stream, 1))
        result = {'protocol': body.number('<H'), 'version': body.text(),
                  'dirty': body.number('<B'), 'channels': body.number('<B'),
                  'board': body.text(), 'hardware_id': body.take(6).hex(), 'label': body.text()}
        body.end()
        return result

    def doctor(self, child):
        check(child.poll() is None, 'owned simulator process exited')
        with self.connect() as stream:
            hello = self.hello(stream)
        check(hello['protocol'] == 3 and hello['board'] == 'sim' and hello['channels'] == 16,
              f'wrong simulator identity: {hello}')
        expected_version = (ROOT / 'Cargo.toml').read_text().split('version = "', 1)[1].split('"', 1)[0]
        check(hello['version'] == expected_version, 'binary version differs from workspace; rebuild')
        return hello

    def state(self, stream, expected):
        actual = self.call(stream, 2)
        check(actual == struct.pack('<H', expected), f'expected Channels {expected:#06x}, got {actual.hex()}')

    def channels(self):
        with self.connect() as stream:
            self.state(stream, 0)
            check(self.call(stream, 3, b'\0' + struct.pack('<HH', 0x8001, 0x8001)) == b'\x01\x80', 'write failed')
            check(self.call(stream, 3, b'\0' + struct.pack('<HH', 1, 0)) == b'\x00\x80', 'mask write lost CH16')
        with self.connect() as stream:
            self.state(stream, 0x8000)
            check(self.call(stream, 4, b'\0') == b'\0\0', 'Clear failed')
            self.state(stream, 0)

    def locks(self):
        with self.connect() as stream:
            check(self.call(stream, 11, text('alice') + b'\0') == b'', 'Lock failed')
        with self.connect() as stream:
            self.call(stream, 3, text('bob') + struct.pack('<HH', 1, 1), status=11)
            self.state(stream, 0)
            check(self.call(stream, 11, text('bob') + b'\1') == b'', 'Takeover failed')
            self.state(stream, 0)
            self.call(stream, 3, text('alice') + struct.pack('<HH', 1, 1), status=11)
            check(self.call(stream, 3, text('bob') + struct.pack('<HH', 1, 1)) == b'\1\0', 'Holder write failed')
            check(self.call(stream, 12, text('bob')) == b'', 'ReleaseLock failed')
            self.state(stream, 1)
            self.call(stream, 4, b'\0')
            self.state(stream, 0)

    def identity(self):
        with self.connect() as stream:
            original = self.hello(stream)
            check(self.call(stream, 6, b'\0' + text('verification-bench')) == text('verification-bench'), 'SetLabel failed')
        with self.connect() as stream:
            renamed = self.hello(stream)
            check(renamed['label'] == 'verification-bench', 'Label did not survive reconnect')
            check(renamed['hardware_id'] == original['hardware_id'], 'rename changed Hardware ID')
            check(self.call(stream, 6, b'\0' + text(original['label'])) == text(original['label']), 'Label restore failed')
            check(self.hello(stream)['label'] == original['label'], 'restored Label readback failed')

    def network_state(self, stream):
        body = Reader(self.call(stream, 5))
        body.take(8 + 1)  # uptime and reset reason
        body.text()  # last panic
        body.take(6 * 4)  # counters
        body.text()  # Holder
        result = (body.number('<B'), body.text(), body.number('<B'), body.take(4),
                  body.number('<B'), body.take(4), body.number('<B'), body.take(4))
        body.end()
        return result

    def network(self):
        zero = b'\0' * 4
        off = (0, '', 0, zero, 0, zero, 0, zero)
        with self.connect() as stream:
            check(self.network_state(stream) == off, 'unexpected starting network state')
            self.call(stream, 13, b'\0' + text('ExampleRigNet') + text('example-passphrase'))
        with self.connect() as stream:
            check(self.network_state(stream) == (1, 'ExampleRigNet', 2, b'\x7f\0\0\1', 0, zero, 0, zero), 'DHCP status mismatch')
            address, gateway = bytes([192, 0, 2, 10]), bytes([192, 0, 2, 1])
            self.call(stream, 14, b'\0\1' + address + bytes([24]) + gateway)
            check(self.network_state(stream) == (1, 'ExampleRigNet', 2, address, 1, address, 24, gateway), 'static status mismatch')
            self.call(stream, 14, b'\0\0' + zero + b'\0' + zero)
            self.call(stream, 13, b'\0\0\0')
            check(self.network_state(stream) == off, 'network configuration did not clear')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('feature', choices=['doctor', 'channels', 'locks', 'identity', 'network', 'all'])
    parser.add_argument('--evidence', type=Path, required=True, help='new directory; retained after cleanup')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/itch-sim')
    args = parser.parse_args()
    args.evidence.mkdir(parents=True, exist_ok=False)
    result = {'feature': args.feature, 'binary': str(args.binary.resolve()), 'passed': False}
    child = None
    try:
        with (args.evidence / 'server.log').open('w') as logs, (args.evidence / 'wire.jsonl').open('w') as transcript:
            command = [str(args.binary.resolve()), '--bind', '127.0.0.1:0']
            result['command'] = command
            child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=logs, text=True)
            result['pid'] = child.pid
            startup = queue.Queue()
            threading.Thread(target=lambda: startup.put(child.stdout.readline()), daemon=True).start()
            line = startup.get(timeout=5).strip()
            result['startup'] = line
            prefix = 'itch-sim listening on tcp://127.0.0.1:'
            check(line.startswith(prefix), f'simulator not ready: {line}')
            driver = Driver(('127.0.0.1', int(line[len(prefix):])), transcript)
            result['doctor'] = driver.doctor(child)
            features = ['channels', 'locks', 'identity', 'network'] if args.feature == 'all' else [args.feature]
            for feature in features:
                if feature != 'doctor':
                    getattr(driver, feature)()
                transcript.write(json.dumps({'feature': feature, 'passed': True}) + '\n')
            result['passed'] = True
    except Exception as error:
        result['error'] = f'{type(error).__name__}: {error}'
    finally:
        if child is not None:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
            child.stdout.close()
            result['cleanup'] = {'process_exited': child.poll() is not None, 'exit_code': child.returncode}
        (args.evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
