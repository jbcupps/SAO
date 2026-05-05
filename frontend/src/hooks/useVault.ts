import { useQuery, useQueryClient } from '@tanstack/react-query';
import {
  configureVault,
  getVaultStatus,
  rotateVaultPassphrase,
  sealVault,
  unsealVault,
} from '../api/vault';
import type {
  ConfigureVaultData,
  RotateVaultPassphraseData,
  VaultLifecycleResponse,
  VaultStatus,
} from '../types';
import { useCallback } from 'react';

export function useVault() {
  const queryClient = useQueryClient();

  const {
    data: vaultStatus,
    isLoading,
    error,
  } = useQuery<VaultStatus>({
    queryKey: ['vault-status'],
    queryFn: getVaultStatus,
    refetchInterval: 30_000,
  });

  const unseal = useCallback(
    async (passphrase: string) => {
      await unsealVault(passphrase);
      await queryClient.invalidateQueries({ queryKey: ['vault-status'] });
    },
    [queryClient],
  );

  const seal = useCallback(async () => {
    await sealVault();
    await queryClient.invalidateQueries({ queryKey: ['vault-status'] });
  }, [queryClient]);

  const configure = useCallback(
    async (data: ConfigureVaultData): Promise<VaultLifecycleResponse> => {
      const result = await configureVault(data);
      await queryClient.invalidateQueries({ queryKey: ['vault-status'] });
      return result;
    },
    [queryClient],
  );

  const rotatePassphrase = useCallback(
    async (
      data: RotateVaultPassphraseData,
    ): Promise<VaultLifecycleResponse> => {
      const result = await rotateVaultPassphrase(data);
      await queryClient.invalidateQueries({ queryKey: ['vault-status'] });
      return result;
    },
    [queryClient],
  );

  return {
    vaultStatus,
    isLoading,
    error,
    unseal,
    seal,
    configure,
    rotatePassphrase,
    isSealed: vaultStatus?.status === 'sealed',
    isUnsealed: vaultStatus?.status === 'unsealed',
    isUninitialized: vaultStatus?.status === 'uninitialized',
    autoUnsealEnvPresent: vaultStatus?.auto_unseal_env_present === true,
  };
}
