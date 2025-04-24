import axios from 'axios';
import qs from 'qs';
import { DataObject, OAuthResponse } from '../../lib/types';

export const refresh = async ({ body }: DataObject): Promise<OAuthResponse> => {
  try {
    // Destructure the properties from the payload.
    const {
      OAUTH_CLIENT_ID: clientId,
      OAUTH_CLIENT_SECRET: clientSecret,
      OAUTH_REFRESH_TOKEN: refreshToken
    } = body;

    // Prepare the refresh request body including client credentials.
    const requestBody = {
      grant_type: 'refresh_token',
      refresh_token: refreshToken,
      client_id: clientId,
      client_secret: clientSecret
    };

    // Make the POST call to Apaleo's token endpoint.
    const response = await axios({
      url: 'https://identity.apaleo.com/connect/token',
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded'
      },
      data: qs.stringify(requestBody)
    });

    // Destructure the relevant fields from the response.
    const {
      access_token: accessToken,
      expires_in: expiresIn,
      token_type: tokenType,
      refresh_token: newRefreshToken
    } = response.data;

    // Return an OAuthResponse-compatible object.
    // If no new refresh token is returned, fallback to the old one.
    return {
      accessToken,
      refreshToken: newRefreshToken || refreshToken,
      expiresIn: +expiresIn,
      tokenType,
      meta: {
        OAUTH_CLIENT_ID: clientId,
        OAUTH_CLIENT_SECRET: clientSecret
      }
    };
  } catch (error) {
    throw new Error(`Error refreshing access token for Apaleo: ${error}`);
  }
};
