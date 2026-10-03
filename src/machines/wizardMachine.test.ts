// Drives the XState wizard mirror with stubbed backend actors, so flows
// and error paths are checked without Tauri or a Proxmark3 attached.

import { describe, expect, it } from 'vitest';
import { createActor, fromPromise, waitFor } from 'xstate';
import { wizardMachine, type WizardContext } from './wizardMachine';
import type { FirmwareCheckResult, WizardState } from './types';

/** Wraps a value an actor should reject with (Error, AppError payload, or "Cancelled"). */
class Reject {
  constructor(readonly error: unknown) {}
}
const reject = (error: unknown) => new Reject(error);

type Stub = WizardState | Reject;

const DEVICE: WizardState = {
  step: 'DeviceConnected',
  data: { port: 'COM7', model: 'PM3 RDV4', firmware: 'v4.20728' },
};

const MATCHED_FW: FirmwareCheckResult = {
  matched: true,
  clientVersion: 'v4.20728',
  deviceFirmwareVersion: 'v4.20728',
  hardwareVariant: 'rdv4',
  firmwarePathExists: true,
};

const EM4100: WizardState = {
  step: 'CardIdentified',
  data: {
    frequency: 'LF',
    card_type: 'EM4100',
    card_data: { uid: '0102030405', raw: '', decoded: {} },
    cloneable: true,
    recommended_blank: 'T5577',
  },
};

const CLASSIC: WizardState = {
  step: 'CardIdentified',
  data: {
    frequency: 'HF',
    card_type: 'MifareClassic1K',
    card_data: { uid: '01020304', raw: '', decoded: {} },
    cloneable: true,
    recommended_blank: 'MagicMifareGen1a',
  },
};

async function settleStub<T>(value: T | Reject): Promise<T> {
  if (value instanceof Reject) throw value.error;
  return value;
}

/** Promise actors that resolve with the stub, or reject for `reject(...)` stubs. */
const stub = <T>(value: T | Reject) => fromPromise<T, WizardContext>(() => settleStub(value));
const stubNoInput = <T>(value: T | Reject) => fromPromise<T>(() => settleStub(value));

function start(stubs: Partial<Record<'detect' | 'scan' | 'blank' | 'write' | 'verify' | 'hf', Stub>> = {}, firmware: FirmwareCheckResult | Reject = MATCHED_FW) {
  const machine = wizardMachine.provide({
    actors: {
      detectDevice: stubNoInput(stubs.detect ?? DEVICE),
      checkFirmware: stub(firmware),
      scanCard: stubNoInput(stubs.scan ?? EM4100),
      detectBlank: stub(
        stubs.blank ?? { step: 'BlankDetected', data: { blank_type: 'T5577', ready_to_write: true, existing_data_type: null } },
      ),
      writeClone: stub(stubs.write ?? ({ step: 'Verifying' } as WizardState)),
      verifyClone: stub(stubs.verify ?? { step: 'VerificationComplete', data: { success: true, mismatched_blocks: [] } }),
      runHfProcess: stub(stubs.hf ?? { step: 'HfDumpReady', data: { dump_info: 'All keys recovered (32/32).' } }),
    },
  });
  const actor = createActor(machine);
  actor.start();
  return actor;
}

type Actor = ReturnType<typeof start>;
type StateName = Parameters<ReturnType<Actor['getSnapshot']>['matches']>[0];
const settle = (actor: Actor, state: StateName) => waitFor(actor, (s) => s.matches(state), { timeout: 1000 });

async function connect(actor: Actor) {
  actor.send({ type: 'DETECT' });
  return settle(actor, 'deviceConnected');
}

describe('wizard machine: device detection', () => {
  it('stores device info and passes a matching firmware check', async () => {
    const snap = await connect(start());
    expect(snap.context).toMatchObject({ port: 'COM7', model: 'PM3 RDV4', firmwareStatus: 'matched' });
  });

  it('routes a firmware mismatch to the update step, and skipping keeps the device', async () => {
    const actor = start({}, { ...MATCHED_FW, matched: false, deviceFirmwareVersion: 'v4.19552' });
    actor.send({ type: 'DETECT' });
    await settle(actor, 'firmwareOutdated');
    actor.send({ type: 'SKIP_FIRMWARE' });
    const snap = await settle(actor, 'deviceConnected');
    expect(snap.context.port).toBe('COM7');
    expect(snap.context.firmwareStatus).toBe('mismatched');
  });

  it('treats a failed firmware check as non-blocking', async () => {
    const actor = start({}, reject(new Error('hw version timed out')));
    const snap = await connect(actor);
    expect(snap.context.firmwareStatus).toBe('unknown');
  });

  it('shows the backend user message when no device is found', async () => {
    const actor = start({
      detect: {
        step: 'Error',
        data: {
          message: 'PM3 not found on any port',
          user_message: 'No Proxmark3 device found. Check your USB connection.',
          recoverable: true,
          recovery_action: 'Retry',
        },
      },
    });
    actor.send({ type: 'DETECT' });
    const snap = await settle(actor, 'error');
    expect(snap.context.errorSource).toBe('detect');
    expect(snap.context.errorUserMessage).toBe('No Proxmark3 device found. Check your USB connection.');
  });

  it('strips local paths from rejected detection errors', async () => {
    const actor = start({ detect: reject(new Error('Failed to spawn proxmark3 (/home/alice/pm3/proxmark3): denied')) });
    actor.send({ type: 'DETECT' });
    const snap = await settle(actor, 'error');
    expect(snap.context.errorMessage).toBe('Failed to spawn proxmark3 ([path]): denied');
  });
});

describe('wizard machine: LF clone', () => {
  it('runs scan -> blank -> write -> verify -> complete', async () => {
    const actor = start();
    await connect(actor);
    actor.send({ type: 'SCAN' });
    await settle(actor, 'cardIdentified');
    actor.send({ type: 'SKIP_TO_BLANK', expectedBlank: 'T5577' });
    await settle(actor, 'blankDetected');
    actor.send({ type: 'WRITE' });
    await settle(actor, 'verificationComplete');
    actor.send({ type: 'FINISH' });
    const snap = await settle(actor, 'complete');
    expect(snap.context.completionTimestamp).not.toBeNull();
    expect(snap.context.cardType).toBe('EM4100');
  });

  it('refuses to write when the blank is not ready', async () => {
    const actor = start({
      blank: { step: 'BlankDetected', data: { blank_type: 'T5577', ready_to_write: false, existing_data_type: 'EM4100' } },
    });
    await connect(actor);
    actor.send({ type: 'SCAN' });
    await settle(actor, 'cardIdentified');
    actor.send({ type: 'SKIP_TO_BLANK', expectedBlank: 'T5577' });
    await settle(actor, 'blankDetected');
    actor.send({ type: 'WRITE' });
    expect(actor.getSnapshot().matches('blankDetected')).toBe(true);
  });

  it('does not finish on a failed verification', async () => {
    const actor = start({ verify: { step: 'VerificationComplete', data: { success: false, mismatched_blocks: [3] } } });
    await connect(actor);
    actor.send({ type: 'SCAN' });
    await settle(actor, 'cardIdentified');
    actor.send({ type: 'SKIP_TO_BLANK', expectedBlank: 'T5577' });
    await settle(actor, 'blankDetected');
    actor.send({ type: 'WRITE' });
    const snap = await settle(actor, 'verificationComplete');
    expect(snap.context.mismatchedBlocks).toEqual([3]);
    actor.send({ type: 'FINISH' });
    expect(actor.getSnapshot().matches('verificationComplete')).toBe(true);
  });

  it('BACK_TO_SCAN clears the card but keeps the device', async () => {
    const actor = start();
    await connect(actor);
    actor.send({ type: 'SCAN' });
    await settle(actor, 'cardIdentified');
    actor.send({ type: 'BACK_TO_SCAN' });
    const snap = await settle(actor, 'deviceConnected');
    expect(snap.context.cardType).toBeNull();
    expect(snap.context.port).toBe('COM7');
  });
});

describe('wizard machine: HF key recovery', () => {
  async function toHfProcessing(actor: Actor) {
    await connect(actor);
    actor.send({ type: 'SCAN' });
    await settle(actor, 'cardIdentified');
    actor.send({ type: 'START_HF_PROCESS' });
  }

  it('reaches the dump-ready step', async () => {
    const actor = start({ scan: CLASSIC });
    await toHfProcessing(actor);
    const snap = await settle(actor, 'hfDumpReady');
    expect(snap.context.hfDumpInfo).toBe('All keys recovered (32/32).');
  });

  it('treats a cancelled autopwn like CANCEL_HF instead of an error', async () => {
    const actor = start({ scan: CLASSIC, hf: reject('Cancelled') });
    await toHfProcessing(actor);
    const snap = await settle(actor, 'deviceConnected');
    expect(snap.context.errorMessage).toBeNull();
    expect(snap.context.cardType).toBeNull();
    expect(snap.context.port).toBe('COM7');
  });

  it('still reports real autopwn failures', async () => {
    const actor = start({ scan: CLASSIC, hf: reject({ CommandFailed: 'PM3 exited with code 1' }) });
    await toHfProcessing(actor);
    const snap = await settle(actor, 'error');
    expect(snap.context.errorUserMessage).toBe('HF key recovery or dump failed.');
    expect(snap.context.errorSource).toBe('scan');
  });
});
