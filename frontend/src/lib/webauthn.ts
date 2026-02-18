import {
  startRegistration,
  startAuthentication,
} from '@simplewebauthn/browser';
import type {
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
} from '@simplewebauthn/types';

export async function beginRegistration(
  options: PublicKeyCredentialCreationOptionsJSON,
) {
  return startRegistration(options);
}

export async function beginAuthentication(
  options: PublicKeyCredentialRequestOptionsJSON,
) {
  return startAuthentication(options);
}
