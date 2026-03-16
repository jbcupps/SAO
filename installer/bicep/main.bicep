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

@description('Optional public browser origin. Leave blank to use the default Container App hostname.')
param publicOrigin string = ''

@description('Optional comma-separated browser origins. Leave blank to allow the resolved public origin only.')
param allowedOrigins string = ''

@description('Optional stable JWT signing secret. Leave blank to persist the signing key under SAO_DATA_DIR instead.')
@secure()
param jwtSecret string = ''

@description('Optional Entra issuer URL used to seed browser sign-in on first boot.')
param oidcIssuerUrl string = ''

@description('Optional Entra client ID used to seed browser sign-in on first boot.')
param oidcClientId string = ''

@description('Optional Entra client secret used to seed browser sign-in on first boot.')
@secure()
param oidcClientSecret string = ''

@description('Display name for the seeded OIDC provider.')
param oidcProviderName string = 'Microsoft Entra ID'

@description('OIDC scopes for the seeded provider.')
param oidcScopes string = 'openid profile email'

var normalizedSuffix = toLower(nameSuffix)
var uniqueToken = toLower(uniqueString(resourceGroup().id))
var compactBaseName = 'sao${uniqueToken}${normalizedSuffix}'
var baseName = empty(normalizedSuffix) ? 'sao-${uniqueToken}' : 'sao-${uniqueToken}-${normalizedSuffix}'
var postgresServerName = '${take(compactBaseName, 52)}-pg'
var keyVaultName = '${take(compactBaseName, 20)}-kv'
var envName = '${take(compactBaseName, 20)}-env'
var storageAccountName = take('${compactBaseName}data', 24)
var resolvedSaoImage = empty(saoImage) ? 'ghcr.io/jbcupps/sao:${saoImageTag}' : saoImage

module network 'modules/network.bicep' = {
  name: 'network'
  params: {
    location: location
    baseName: baseName
  }
}

module postgres 'modules/postgres.bicep' = {
  name: 'postgres'
  params: {
    location: location
    serverName: postgresServerName
    adminPassword: pgAdminPassword
    delegatedSubnetId: network.outputs.postgresSubnetId
    privateDnsZoneId: network.outputs.postgresPrivateDnsZoneId
  }
}

module keyVault 'modules/keyvault.bicep' = {
  name: 'keyvault'
  params: {
    location: location
    vaultName: keyVaultName
    adminOid: adminOid
  }
}

module containerApp 'modules/container-app.bicep' = {
  name: 'container-app'
  params: {
    location: location
    appName: 'sao-app'
    envName: envName
    saoImage: resolvedSaoImage
    pgServerFqdn: postgres.outputs.serverFqdn
    pgAdminPassword: pgAdminPassword
    adminOid: adminOid
    infrastructureSubnetId: network.outputs.containerAppsSubnetId
    storageAccountName: storageAccountName
    publicOrigin: publicOrigin
    allowedOrigins: allowedOrigins
    jwtSecret: jwtSecret
    oidcIssuerUrl: oidcIssuerUrl
    oidcClientId: oidcClientId
    oidcClientSecret: oidcClientSecret
    oidcProviderName: oidcProviderName
    oidcScopes: oidcScopes
  }
}

output saoEndpoint string = containerApp.outputs.fqdn
output publicOrigin string = containerApp.outputs.publicOrigin
output resourceGroupName string = resourceGroup().name
