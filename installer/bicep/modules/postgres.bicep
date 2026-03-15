param location string
param serverName string
@secure()
param adminPassword string

resource pgServer 'Microsoft.DBforPostgreSQL/flexibleServers@2023-12-01-preview' = {
  name: serverName
  location: location
  sku: {
    name: 'Standard_B1ms'
    tier: 'Burstable'
  }
  properties: {
    version: '16'
    storage: { storageSizeGB: 32 }
    administratorLogin: 'saoadmin'
    administratorLoginPassword: adminPassword
    highAvailability: { mode: 'Disabled' }
  }
}

resource pgDatabase 'Microsoft.DBforPostgreSQL/flexibleServers/databases@2023-12-01-preview' = {
  parent: pgServer
  name: 'sao'
  properties: { charset: 'UTF8', collation: 'en_US.utf8' }
}

resource pgFirewall 'Microsoft.DBforPostgreSQL/flexibleServers/firewallRules@2023-12-01-preview' = {
  parent: pgServer
  name: 'AllowAzureServices'
  properties: {
    startIpAddress: '0.0.0.0'
    endIpAddress: '0.0.0.0'
  }
}

output connectionString string = 'postgresql://saoadmin:${adminPassword}@${pgServer.properties.fullyQualifiedDomainName}:5432/sao?sslmode=require'
