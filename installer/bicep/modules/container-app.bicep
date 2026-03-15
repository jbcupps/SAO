param location string
param appName string
param envName string
param saoImageTag string
param databaseUrl string
param keyVaultUri string
param adminOid string

resource logAnalytics 'Microsoft.OperationalInsights/workspaces@2023-09-01' = {
  name: '${envName}-logs'
  location: location
  properties: { sku: { name: 'PerGB2018' }, retentionInDays: 30 }
}

resource containerAppEnv 'Microsoft.App/managedEnvironments@2024-03-01' = {
  name: envName
  location: location
  properties: {
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logAnalytics.properties.customerId
        sharedKey: logAnalytics.listKeys().primarySharedKey
      }
    }
  }
}

resource saoApp 'Microsoft.App/containerApps@2024-03-01' = {
  name: appName
  location: location
  identity: { type: 'SystemAssigned' }
  properties: {
    managedEnvironmentId: containerAppEnv.id
    configuration: {
      ingress: {
        external: true
        targetPort: 3100
        transport: 'http'
      }
    }
    template: {
      containers: [
        {
          name: 'sao'
          image: 'ghcr.io/jbcupps/sao:${saoImageTag}'
          resources: { cpu: json('0.5'), memory: '1Gi' }
          env: [
            { name: 'DATABASE_URL', value: databaseUrl }
            { name: 'KEY_VAULT_URI', value: keyVaultUri }
            { name: 'SAO_BOOTSTRAP_ADMIN_OID', value: adminOid }
            { name: 'SAO_PORT', value: '3100' }
          ]
        }
      ]
      scale: { minReplicas: 1, maxReplicas: 1 }
    }
  }
}

output fqdn string = saoApp.properties.configuration.ingress.fqdn
