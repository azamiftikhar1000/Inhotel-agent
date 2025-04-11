import axios from 'axios';
import qs from 'qs';
import { DataObject, OAuthResponse } from '../../lib/types';
import { generateBasicHeaders } from '../../lib/helpers';

export const refresh = async ({ body }: DataObject): Promise<OAuthResponse> => {
  try {
    // Destructure the properties from the payload
    const {
      OAUTH_CLIENT_ID: client_id,
      OAUTH_CLIENT_SECRET: client_secret,
      OAUTH_REFRESH_TOKEN: refresh_token,
      OAUTH_REQUEST_PAYLOAD: {
        formData, // if you have additional fields inside formData, they are accessible here
      },
    } = body;

    // Prepare the refresh request body
    const requestBody = {
      grant_type: 'refresh_token',
      refresh_token,
    };

    // Make the POST call to Apaleo's token endpoint
    const response = await axios({
      url: 'https://identity.apaleo.com/connect/token',
      method: 'POST',
      headers: generateBasicHeaders(client_id, client_secret),
      data: qs.stringify(requestBody),
    });

    // Destructure the relevant fields from the response
    const {
      access_token: accessToken,
      expires_in: expiresIn,
      token_type: tokenType,
      refresh_token: newRefreshToken,
    } = response.data;

    // Return an OAuthResponse-compatible object
    // Note: Apaleo typically returns a new refresh token each time.
    return {
      accessToken,
      // Fallback to the old refresh token if none is returned
      refreshToken: newRefreshToken || refresh_token,
      expiresIn: +expiresIn,
      tokenType,
      meta: {
        ...body?.OAUTH_METADATA?.meta, // keep or omit as needed
      },
    };
  } catch (error) {
    throw new Error(`Error refreshing access token for Apaleo: ${error}`);
  }
};
