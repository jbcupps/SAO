param location string
param vaultName string
param adminOid string

resource vault 'Microsoft.KeyVault/vaults@2023-07-01' = {
  name: vaultName
  location: location
  properties: {
    sku: { family: 'A', name: 'standard' }
    tenantId: subscription().tenantId
    accessPolicies: [
      {
        objectId: adminOid
        tenantId: subscription().tenantId
        permissions: {
          secrets: ['get', 'set', 'list', 'delete']
          keys: ['get', 'create', 'list', 'sign', 'verify']
        }
      }
    ]
  }
}

output vaultUri string = vault.properties.vaultUri
