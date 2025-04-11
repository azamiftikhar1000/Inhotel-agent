import axios from 'axios';
import qs from 'qs';
import { DataObject, OAuthResponse } from '../../lib/types';
import { generateBasicHeaders } from '../../lib/helpers';

export const init = async ({ body }: DataObject): Promise<OAuthResponse> => {
  try {
    // Destructure the properties from the payload
    const {
      clientId: client_id,
      clientSecret: client_secret,
      metadata: {
        code,
        formData, // in case you have additional fields inside formData
        redirectUri: redirect_uri,
      },
    } = body;

    // Prepare the token request body
    const requestBody = {
      grant_type: 'authorization_code',
      code,
      redirect_uri,
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
      refresh_token: refreshToken,
      expires_in: expiresIn,
      token_type: tokenType,
    } = response.data;

    // Return an OAuthResponse-compatible object
    return {
      accessToken,
      refreshToken,
      expiresIn: +expiresIn,
      tokenType,
      meta: {}, // or any other metadata you want to store
    };
  } catch (error) {
    throw new Error(`Error fetching access token for Apaleo: ${error}`);
  }
};
