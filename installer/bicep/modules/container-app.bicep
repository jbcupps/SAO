param location string
param appName string
param envName string
param saoImage string
param pgServerFqdn string
@secure()
param pgAdminPassword string
param keyVaultUri string
param adminOid string

var databaseUrl = 'postgresql://saoadmin:${pgAdminPassword}@${pgServerFqdn}:5432/sao?sslmode=require'

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
      secrets: [
        {
          name: 'database-url'
          #disable-next-line use-secure-value-for-secure-inputs // Built from a secure password param and only written into Container Apps secrets.
          value: databaseUrl
        }
      ]
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
          image: saoImage
          resources: { cpu: json('0.5'), memory: '1Gi' }
          env: [
            { name: 'DATABASE_URL', secretRef: 'database-url' }
            { name: 'KEY_VAULT_URI', value: keyVaultUri }
            { name: 'RUST_BACKTRACE', value: '1' }
            { name: 'SAO_BOOTSTRAP_ADMIN_OID', value: adminOid }
            { name: 'SAO_PORT', value: '3100' }
            { name: 'SAO_STARTUP_DB_MAX_WAIT_SECONDS', value: '75' }
          ]
          probes: [
            {
              type: 'Startup'
              httpGet: {
                path: '/api/health'
                port: 3100
              }
              initialDelaySeconds: 5
              periodSeconds: 5
              failureThreshold: 18
            }
            {
              type: 'Readiness'
              httpGet: {
                path: '/api/health'
                port: 3100
              }
              initialDelaySeconds: 10
              periodSeconds: 10
              failureThreshold: 6
            }
            {
              type: 'Liveness'
              httpGet: {
                path: '/api/health'
                port: 3100
              }
              initialDelaySeconds: 30
              periodSeconds: 15
              failureThreshold: 3
            }
          ]
        }
      ]
      scale: { minReplicas: 1, maxReplicas: 1 }
    }
  }
}

output fqdn string = saoApp.properties.configuration.ingress.fqdn
