import { useQuery, useQueryClient } from '@tanstack/react-query';
import { getVaultStatus, unsealVault, sealVault } from '../api/vault';
import type { VaultStatus } from '../types';
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

  return {
    vaultStatus,
    isLoading,
    error,
    unseal,
    seal,
    isSealed: vaultStatus?.status === 'sealed',
    isUnsealed: vaultStatus?.status === 'unsealed',
    isUninitialized: vaultStatus?.status === 'uninitialized',
  };
}
