import {
  startRegistration,
  startAuthentication,
} from '@simplewebauthn/browser';
import type {
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
} from '@simplewebauthn/types';

export async function beginRegistration(
  options: PublicKeyCredentialCreationOptionsJSON | { publicKey: PublicKeyCredentialCreationOptionsJSON },
) {
  return startRegistration(unwrapPublicKeyOptions(options));
}

export async function beginAuthentication(
  options: PublicKeyCredentialRequestOptionsJSON | { publicKey: PublicKeyCredentialRequestOptionsJSON },
) {
  return startAuthentication(unwrapPublicKeyOptions(options));
}

function unwrapPublicKeyOptions<T>(options: T | { publicKey: T }): T {
  if (
    typeof options === 'object' &&
    options !== null &&
    'publicKey' in options
  ) {
    return options.publicKey;
  }

  return options;
}
