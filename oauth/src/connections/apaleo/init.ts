import axios from 'axios';
import qs from 'qs';
import { DataObject, OAuthResponse } from '../../lib/types';

export const init = async ({ body }: DataObject): Promise<OAuthResponse> => {
  // Log complete environment and configuration information
  console.log('Apaleo OAuth Environment:', {
    NODE_ENV: process.env.NODE_ENV,
    availableEnvVars: Object.keys(process.env).filter(key => 
      key.includes('APALEO') || key.includes('OAUTH') || key.includes('SECRET')
    ).map(key => `${key}: ${key.includes('SECRET') ? 'REDACTED' : 'PRESENT'}`)
  });

  try {
    // Log incoming body structure (not just values)
    console.log('Apaleo OAuth init - Complete body structure:', {
      keys: Object.keys(body),
      hasClientId: !!body.clientId,
      hasClientSecret: !!body.clientSecret,
      metadataKeys: body.metadata ? Object.keys(body.metadata) : 'metadata missing',
      hasCode: body.metadata?.code ? true : false,
      hasRedirectUri: body.metadata?.redirectUri ? true : false
    });

    // Log incoming body (with sensitive data masked)
    console.log('Apaleo OAuth init - Request body:', {
      ...body,
      clientSecret: body.clientSecret ? '***REDACTED***' : undefined,
      metadata: body.metadata ? {
        ...body.metadata,
        code: body.metadata.code ? '***REDACTED***' : undefined
      } : undefined
    });

    // Destructure the properties from the payload.
    const {
      clientId,
      clientSecret,
      metadata: { code, redirectUri } // note: using camelCase for consistency
    } = body;

    // Prepare the token request body with client credentials included.
    const requestBody = {
      grant_type: 'authorization_code',
      code,
      redirect_uri: redirectUri,
      client_id: clientId,
      client_secret: clientSecret
    };

    // Log the request details (with sensitive data masked)
    console.log('Apaleo OAuth init - Preparing token request:', {
      url: 'https://identity.apaleo.com/connect/token',
      method: 'POST',
      requestBody: {
        ...requestBody,
        code: '***REDACTED***',
        client_secret: '***REDACTED***'
      }
    });

    // Make the POST call to Apaleo's token endpoint.
    const response = await axios({
      url: 'https://identity.apaleo.com/connect/token',
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded'
      },
      data: qs.stringify(requestBody)
    });

    // Log successful response (with tokens redacted)
    console.log('Apaleo OAuth init - Token response received:', {
      status: response.status,
      statusText: response.statusText,
      data: {
        ...response.data,
        access_token: response.data.access_token ? '***REDACTED***' : undefined,
        refresh_token: response.data.refresh_token ? '***REDACTED***' : undefined
      }
    });

    // Destructure the relevant fields from the response.
    const {
      access_token: accessToken,
      refresh_token: refreshToken,
      expires_in: expiresIn,
      token_type: tokenType
    } = response.data;

    // Return an OAuthResponse-compatible object.
    return {
      accessToken,
      refreshToken,
      expiresIn: +expiresIn,
      tokenType,
      meta: {
        OAUTH_CLIENT_ID: clientId,
        OAUTH_CLIENT_SECRET: clientSecret
      }
    };
  } catch (error: unknown) {
    // Detailed error logging with proper type checking
    console.error('Apaleo OAuth init - Error details:', {
      message: error instanceof Error ? error.message : String(error),
      status: axios.isAxiosError(error) ? error.response?.status : undefined,
      statusText: axios.isAxiosError(error) ? error.response?.statusText : undefined,
      responseData: axios.isAxiosError(error) ? error.response?.data : undefined,
      requestConfig: axios.isAxiosError(error) && error.config ? {
        url: error.config.url,
        method: error.config.method,
        headers: {
          ...(error.config.headers as Record<string, unknown>),
          Authorization: (error.config.headers as Record<string, unknown>)?.Authorization 
            ? '***REDACTED***' 
            : undefined
        }
      } : undefined
    });
    
    throw new Error(`Error fetching access token for Apaleo: ${error instanceof Error ? error.message : String(error)}`);
  }
};
