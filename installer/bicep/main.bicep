@description('Azure region for all resources')
param location string

@description('Entra Object ID of the bootstrap admin')
param adminOid string

@description('SAO container image tag')
param saoImageTag string = 'latest'

@description('Optional full SAO container image reference override')
param saoImage string = ''

@description('Optional short suffix used to avoid global name collisions')
@maxLength(3)
param nameSuffix string = ''

@description('PostgreSQL admin password')
@secure()
param pgAdminPassword string = newGuid()

var normalizedSuffix = empty(nameSuffix) ? '' : '-${toLower(nameSuffix)}'
var baseName = 'sao-${uniqueString(resourceGroup().id)}${normalizedSuffix}'
var resolvedSaoImage = empty(saoImage) ? 'ghcr.io/jbcupps/sao:${saoImageTag}' : saoImage

module postgres 'modules/postgres.bicep' = {
  name: 'postgres'
  params: {
    location: location
    serverName: '${baseName}-pg'
    adminPassword: pgAdminPassword
  }
}

module keyVault 'modules/keyvault.bicep' = {
  name: 'keyvault'
  params: {
    location: location
    vaultName: '${baseName}-kv'
    adminOid: adminOid
  }
}

module containerApp 'modules/container-app.bicep' = {
  name: 'container-app'
  params: {
    location: location
    appName: 'sao-app'
    envName: '${baseName}-env'
    saoImage: resolvedSaoImage
    databaseUrl: postgres.outputs.connectionString
    keyVaultUri: keyVault.outputs.vaultUri
    adminOid: adminOid
  }
}

output saoEndpoint string = containerApp.outputs.fqdn
output resourceGroupName string = resourceGroup().name
