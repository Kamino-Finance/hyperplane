import {UpdatePoolConfigValueKind} from '../_generated/hyperplane-client/types';

export function serializeConfigValue(val: UpdatePoolConfigValueKind): number[] {
  let buffer: Buffer;
  switch (val.kind) {
    case 'Bool': {
      buffer = Buffer.alloc(32);
      val.value[0] ? buffer.writeUInt8(1, 0) : buffer.writeUInt8(0, 0);
    }
  }
  return [...buffer];
}
