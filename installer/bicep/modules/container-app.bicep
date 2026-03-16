param location string
param appName string
param envName string
param saoImage string
param pgServerFqdn string
@secure()
param pgAdminPassword string
param adminOid string
param infrastructureSubnetId string
param storageAccountName string
param publicOrigin string = ''
param allowedOrigins string = ''
@secure()
param jwtSecret string = ''
param oidcIssuerUrl string = ''
param oidcClientId string = ''
@secure()
param oidcClientSecret string = ''
param oidcProviderName string = 'Microsoft Entra ID'
param oidcScopes string = 'openid profile email'

var databaseUrl = 'postgresql://saoadmin:${pgAdminPassword}@${pgServerFqdn}:5432/sao?sslmode=require'

resource logAnalytics 'Microsoft.OperationalInsights/workspaces@2023-09-01' = {
  name: '${envName}-logs'
  location: location
  properties: {
    sku: {
      name: 'PerGB2018'
    }
    retentionInDays: 30
  }
}

resource storageAccount 'Microsoft.Storage/storageAccounts@2023-05-01' = {
  name: storageAccountName
  location: location
  sku: {
    name: 'Standard_LRS'
  }
  kind: 'StorageV2'
  properties: {
    accessTier: 'Hot'
    allowBlobPublicAccess: false
    allowSharedKeyAccess: true
    minimumTlsVersion: 'TLS1_2'
    supportsHttpsTrafficOnly: true
  }
}

resource fileService 'Microsoft.Storage/storageAccounts/fileServices@2023-05-01' = {
  parent: storageAccount
  name: 'default'
}

resource dataShare 'Microsoft.Storage/storageAccounts/fileServices/shares@2023-05-01' = {
  parent: fileService
  name: 'sao-data'
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
    vnetConfiguration: {
      infrastructureSubnetId: infrastructureSubnetId
      internal: false
    }
  }
}

resource environmentStorage 'Microsoft.App/managedEnvironments/storages@2024-03-01' = {
  parent: containerAppEnv
  name: 'saodata'
  properties: {
    azureFile: {
      accountName: storageAccount.name
      accountKey: storageAccount.listKeys().keys[0].value
      shareName: dataShare.name
      accessMode: 'ReadWrite'
    }
  }
}

var defaultPublicOrigin = 'https://${appName}.${containerAppEnv.properties.defaultDomain}'
var resolvedPublicOrigin = empty(publicOrigin) ? defaultPublicOrigin : publicOrigin
var resolvedAllowedOrigins = empty(allowedOrigins) ? resolvedPublicOrigin : allowedOrigins
var resolvedRpId = replace(replace(replace(resolvedPublicOrigin, 'https://', ''), 'http://', ''), '/', '')
var appSecrets = concat(
  [
    {
      name: 'database-url'
      value: databaseUrl
    }
  ],
  empty(jwtSecret) ? [] : [
    {
      name: 'jwt-secret'
      value: jwtSecret
    }
  ],
  empty(oidcClientSecret) ? [] : [
    {
      name: 'oidc-client-secret'
      value: oidcClientSecret
    }
  ]
)
var appEnv = concat(
  [
    {
      name: 'DATABASE_URL'
      secretRef: 'database-url'
    }
    {
      name: 'RUST_LOG'
      value: 'info'
    }
    {
      name: 'SAO_BIND_ADDR'
      value: '0.0.0.0:3100'
    }
    {
      name: 'SAO_DATA_DIR'
      value: '/data/sao'
    }
    {
      name: 'SAO_BOOTSTRAP_ADMIN_OID'
      value: adminOid
    }
    {
      name: 'SAO_FRONTEND_URL'
      value: resolvedPublicOrigin
    }
    {
      name: 'SAO_ALLOWED_ORIGINS'
      value: resolvedAllowedOrigins
    }
    {
      name: 'SAO_COOKIE_SECURE'
      value: 'true'
    }
    {
      name: 'SAO_RP_ID'
      value: resolvedRpId
    }
    {
      name: 'SAO_RP_ORIGIN'
      value: resolvedPublicOrigin
    }
  ],
  empty(jwtSecret) ? [] : [
    {
      name: 'SAO_JWT_SECRET'
      secretRef: 'jwt-secret'
    }
  ],
  empty(oidcIssuerUrl) || empty(oidcClientId) ? [] : [
    {
      name: 'SAO_OIDC_ISSUER_URL'
      value: oidcIssuerUrl
    }
    {
      name: 'SAO_OIDC_CLIENT_ID'
      value: oidcClientId
    }
    {
      name: 'SAO_OIDC_PROVIDER_NAME'
      value: oidcProviderName
    }
    {
      name: 'SAO_OIDC_SCOPES'
      value: oidcScopes
    }
  ],
  empty(oidcClientSecret) ? [] : [
    {
      name: 'SAO_OIDC_CLIENT_SECRET'
      secretRef: 'oidc-client-secret'
    }
  ]
)

resource saoApp 'Microsoft.App/containerApps@2024-03-01' = {
  name: appName
  location: location
  identity: {
    type: 'SystemAssigned'
  }
  properties: {
    managedEnvironmentId: containerAppEnv.id
    configuration: {
      activeRevisionsMode: 'Single'
      ingress: {
        external: true
        targetPort: 3100
        transport: 'auto'
        allowInsecure: false
      }
      secrets: appSecrets
    }
    template: {
      containers: [
        {
          name: 'sao'
          image: saoImage
          resources: {
            cpu: json('0.5')
            memory: '1Gi'
          }
          env: appEnv
          volumeMounts: [
            {
              volumeName: 'sao-data'
              mountPath: '/data/sao'
            }
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
      volumes: [
        {
          name: 'sao-data'
          storageType: 'AzureFile'
          storageName: environmentStorage.name
        }
      ]
      scale: {
        minReplicas: 1
        maxReplicas: 1
      }
    }
  }
}

output fqdn string = empty(publicOrigin) ? '${appName}.${containerAppEnv.properties.defaultDomain}' : replace(replace(publicOrigin, 'https://', ''), 'http://', '')
output publicOrigin string = resolvedPublicOrigin
