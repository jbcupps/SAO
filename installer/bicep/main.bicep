@description('Azure region for all resources')
param location string

@description('Entra Object ID of the bootstrap admin')
param adminOid string

@description('SAO container image tag')
param saoImageTag string = 'latest'

@description('PostgreSQL admin password')
@secure()
param pgAdminPassword string = newGuid()

var baseName = 'sao-${uniqueString(resourceGroup().id)}'

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
    saoImageTag: saoImageTag
    databaseUrl: postgres.outputs.connectionString
    keyVaultUri: keyVault.outputs.vaultUri
    adminOid: adminOid
  }
}

output saoEndpoint string = containerApp.outputs.fqdn
output resourceGroupName string = resourceGroup().name
